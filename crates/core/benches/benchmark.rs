#![allow(clippy::needless_return)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use criterion::{Bencher, Criterion, Throughput, criterion_group, criterion_main};

use axum::body::Body;
use axum::extract::{Json, State};
use axum::http::{self, Request};
use base64::prelude::*;
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use hyper::StatusCode;
use std::time::{Duration, Instant};
use tower::{Service, ServiceExt};

use trailbase::api::{
  CreateUserRequest, InitArgs, create_user_handler, login_with_password_for_test,
};
use trailbase::config::proto::{PermissionFlag, RecordApiConfig};
use trailbase::constants::RECORD_API_PATH;
use trailbase::{AppState, SocketAddr};
use trailbase::{DataDir, Server, ServerOptions};
use trailbase_sqlite::params;

async fn create_chat_message_app_tables(
  conn: &trailbase_sqlite::Connection,
) -> Result<(), trailbase_sqlite::Error> {
  // Create a messages, chat room and members tables.
  conn
    .execute_batch(
      r#"
          CREATE TABLE room (
            id           BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
            name         TEXT
          ) STRICT;

          CREATE TABLE message (
            id           INTEGER PRIMARY KEY,
            _owner       BLOB NOT NULL,
            room         BLOB NOT NULL,
            data         TEXT NOT NULL DEFAULT 'empty',

            -- on user delete, toombstone it.
            FOREIGN KEY(_owner) REFERENCES _user(id) ON DELETE SET NULL,
            -- On chatroom delete, delete message
            FOREIGN KEY(room) REFERENCES room(id) ON DELETE CASCADE
          ) STRICT;

          CREATE TABLE room_members (
            user         BLOB NOT NULL,
            room         BLOB NOT NULL,

            FOREIGN KEY(room) REFERENCES room(id) ON DELETE CASCADE,
            FOREIGN KEY(user) REFERENCES _user(id) ON DELETE CASCADE
          ) STRICT;
        "#,
    )
    .await?;

  return Ok(());
}

async fn add_room(
  conn: &trailbase_sqlite::Connection,
  name: &str,
) -> Result<[u8; 16], anyhow::Error> {
  let room: [u8; 16] = conn
    .write_query_row_get(
      "INSERT INTO room (name) VALUES ($1) RETURNING id",
      params!(name.to_string()),
      0,
    )
    .await?
    .unwrap();

  return Ok(room);
}

async fn add_user_to_room(
  conn: &trailbase_sqlite::Connection,
  user: [u8; 16],
  room: [u8; 16],
) -> Result<(), trailbase_sqlite::Error> {
  conn
    .execute(
      "INSERT INTO room_members (user, room) VALUES ($1, $2)",
      params!(user, room),
    )
    .await?;
  return Ok(());
}

struct Setup {
  app: Server,

  room: [u8; 16],
  user_x: [u8; 16],
  user_x_token: String,
}

pub(crate) async fn add_record_api_config(
  state: &AppState,
  api: RecordApiConfig,
) -> Result<(), anyhow::Error> {
  let mut config = (*state.get_config()).clone();
  config.record_apis.push(api);
  return Ok(state.validate_and_update_config(config, None).await?);
}

async fn setup_app() -> Result<Setup, anyhow::Error> {
  let data_dir = temp_dir::TempDir::new()?;

  let (_new, state) = AppState::init(InitArgs {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    ..Default::default()
  })
  .await
  .unwrap();

  let app = Server::init(
    state,
    SocketAddr::parse("localhost:4020").unwrap(),
    ServerOptions {
      ..Default::default()
    },
  )
  .await?;

  let main_conn = app.state.connection_manager().main_entry();
  let conn = &main_conn.connection;

  create_chat_message_app_tables(conn).await?;
  app.state.rebuild_connection_metadata().await?;

  let room = add_room(conn, "room0").await?;
  let password = "Secret!1!!";

  let create_access_rule =
    r#"(SELECT 1 FROM room_members WHERE user = _USER_.id AND room = _REQ_.room)"#;

  add_record_api_config(
    &app.state,
    RecordApiConfig {
      name: Some("messages_api".to_string()),
      table_name: Some("message".to_string()),
      acl_authenticated: [PermissionFlag::Read as i32, PermissionFlag::Create as i32].into(),
      create_access_rule: Some(create_access_rule.to_string()),
      ..Default::default()
    },
  )
  .await?;

  let email = "user_x@bar.com";
  let user_x = create_user_handler(
    State(app.state.clone()),
    Json(CreateUserRequest {
      email: Some(email.to_string()),
      username: None,
      password: password.to_string(),
      verified: true,
      admin: false,
    }),
  )
  .await?
  .id
  .into_bytes();

  let user_x_token = login_with_password_for_test(
    &app.state,
    trailbase::api::UserIdentifier::Email(email.to_string()),
    password,
  )
  .await?
  .unwrap()
  .auth_token;

  add_user_to_room(conn, user_x, room).await?;

  return Ok(Setup {
    app,
    room,
    user_x,
    user_x_token,
  });
}

async fn check_health(router: &mut axum::Router<()>) -> Result<(), anyhow::Error> {
  let response = router
    .call(
      Request::builder()
        .method(http::Method::GET)
        .uri("/api/healthcheck")
        .body(Body::from(vec![]))
        .unwrap(),
    )
    .await?;

  if response.status() != StatusCode::OK {
    anyhow::bail!("Expected 'Ok' status");
  }

  let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();

  if bytes.to_vec() != b"Ok" {
    anyhow::bail!("Expected 'Ok'");
  }

  return Ok(());
}

fn create_message_benchmark(b: &mut Bencher, runtime: &tokio::runtime::Runtime, setup: &Setup) {
  let authorization = format!("Bearer {}", setup.user_x_token);
  let body = {
    let request = serde_json::json!({
      "_owner": BASE64_URL_SAFE.encode(setup.user_x),
      "room": BASE64_URL_SAFE.encode(setup.room),
      "data": "user_x message to room",
    });

    serde_json::to_vec(&request).unwrap()
  };

  let request = move || {
    return Request::builder()
      .method(http::Method::POST)
      .uri(&format!("/{RECORD_API_PATH}/messages_api"))
      .header(http::header::CONTENT_TYPE, "application/json")
      .header(http::header::AUTHORIZATION, &authorization)
      .body(Body::from(body.clone()))
      .unwrap();
  };

  b.to_async(runtime).iter_custom(async |iters| {
    let start = Instant::now();

    let tasks = (0..iters).map(|_i| {
      let request = request.clone();
      let mut router = setup.app.main_router.1.clone();

      return runtime.spawn(async move {
        let response = router.call(request()).await.unwrap();
        assert!(response.status().is_success());
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn list_message_benchmark(b: &mut Bencher, runtime: &tokio::runtime::Runtime, setup: &Setup) {
  let authorization = format!("Bearer {}", setup.user_x_token);

  let request = move || {
    return Request::builder()
      .method(http::Method::GET)
      .uri(&format!("/{RECORD_API_PATH}/messages_api?limit=50"))
      .header(http::header::CONTENT_TYPE, "application/json")
      .header(http::header::AUTHORIZATION, &authorization)
      .body(Body::empty())
      .unwrap();
  };

  b.to_async(runtime).iter_custom(async |iters| {
    let start = Instant::now();

    let tasks = (0..iters).map(|_i| {
      let request = request.clone();
      let mut router = setup.app.main_router.1.clone();

      return runtime.spawn(async move {
        let response = router.call(request()).await.unwrap();
        assert!(response.status().is_success());
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn subscribe_message_benchmark(b: &mut Bencher, runtime: &tokio::runtime::Runtime, setup: &Setup) {
  let authorization = format!("Bearer {}", setup.user_x_token);
  let create_request_body = {
    let request = serde_json::json!({
      "_owner": BASE64_URL_SAFE.encode(setup.user_x),
      "room": BASE64_URL_SAFE.encode(setup.room),
      "data": "user_x message to room",
    });

    serde_json::to_vec(&request).unwrap()
  };

  let create_request = {
    let authorization = authorization.clone();
    move || {
      return Request::builder()
        .method(http::Method::POST)
        .uri(&format!("/{RECORD_API_PATH}/messages_api"))
        .header(http::header::CONTENT_TYPE, "application/json")
        .header(http::header::AUTHORIZATION, &authorization)
        .body(Body::from(create_request_body.clone()))
        .unwrap();
    }
  };

  let subscribe_request = move || {
    return Request::builder()
      .method(http::Method::GET)
      .uri(&format!("/{RECORD_API_PATH}/subscribe/*"))
      .header(http::header::CONTENT_TYPE, "application/json")
      .header(http::header::AUTHORIZATION, &authorization)
      .body(Body::empty())
      .unwrap();
  };

  b.to_async(runtime).iter_custom(async |iters| {
    const N_MESSAGES: usize = 100;
    const N_SUBSCRIBERS: usize = 10;

    let start = Instant::now();

    let tasks = (0..iters).map(|_i| {
      let create_request = create_request.clone();
      let subscribe_request = subscribe_request.clone();
      let mut router = setup.app.main_router.1.clone();

      return runtime.spawn(async move {
        let subscriptions: Vec<_> = {
          let mut subscriptions = vec![];
          for _ in 0..N_SUBSCRIBERS {
            subscriptions.push(
              http_body_util::BodyDataStream::new(
                router.call(subscribe_request()).await.unwrap().into_body(),
              )
              .eventsource(),
            );
          }

          subscriptions
        };

        // First publish messages.
        for _ in 0..N_MESSAGES {
          let response = router.call(create_request()).await.unwrap();
          assert!(response.status().is_success());
        }

        // Then listen on all messages for all subscribers.
        futures_util::future::join_all(subscriptions.into_iter().map(async |subscription| {
          let listen_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(10),
            subscription.take(N_MESSAGES).collect::<Vec<_>>(),
          )
          .await;

          assert!(listen_result.is_ok());
        }))
        .await;
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn benchmark_group(c: &mut Criterion) {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(8)
    .enable_all()
    .build()
    .unwrap();

  let setup = runtime.block_on(async {
    let setup = setup_app().await.unwrap();
    let mut router = setup.app.main_router.1.clone();

    ServiceExt::<Request<Body>>::ready(&mut router)
      .await
      .unwrap();

    // Start server and make sure healthcheck returns Ok;
    check_health(&mut router).await.unwrap();

    setup
  });

  {
    let mut group = c.benchmark_group("ChatCreateMessages");
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(100);
    group.throughput(Throughput::Elements(1));

    group.bench_function("single-threaded", |b| {
      let current_thread_runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

      create_message_benchmark(b, &current_thread_runtime, &setup)
    });

    group.bench_function("parallel", |b| {
      create_message_benchmark(b, &runtime, &setup)
    });
  }

  {
    let mut group = c.benchmark_group("ChatListMessages");
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(100);
    group.throughput(Throughput::Elements(1));

    group.bench_function("parallel", |b| list_message_benchmark(b, &runtime, &setup));
  }

  {
    let mut group = c.benchmark_group("ChatSubscribeMessages");
    group.measurement_time(Duration::from_secs(20));
    group.sample_size(10);
    group.throughput(Throughput::Elements(1));

    group.bench_function("parallel", |b| {
      subscribe_message_benchmark(b, &runtime, &setup)
    });
  }
}

criterion_group!(benches, benchmark_group);
criterion_main!(benches);
