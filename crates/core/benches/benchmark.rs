#![allow(clippy::needless_return)]

use criterion::{Bencher, Criterion, Throughput, criterion_group, criterion_main};

use axum::body::Body;
use axum::extract::{Json, State};
use axum::http::{self, Request};
use base64::prelude::*;
use hyper::StatusCode;
use std::time::{Duration, Instant};
use tower::{Service, ServiceExt};

use trailbase::AppState;
use trailbase::api::{CreateUserRequest, create_user_handler, login_with_password};
use trailbase::config::proto::{PermissionFlag, RecordApiConfig};
use trailbase::constants::RECORD_API_PATH;
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
    .query_row_f(
      "INSERT INTO room (name) VALUES ($1) RETURNING id",
      params!(name.to_string()),
      |row| row.get(0),
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
  let mut config = state.get_config();
  config.record_apis.push(api);
  return Ok(state.validate_and_update_config(config, None).await?);
}

async fn setup_app() -> Result<Setup, anyhow::Error> {
  let data_dir = temp_dir::TempDir::new()?;

  let app = Server::init(ServerOptions {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    address: "localhost".to_string(),
    ..Default::default()
  })
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
      email: email.to_string(),
      password: password.to_string(),
      verified: true,
      admin: false,
    }),
  )
  .await?
  .id
  .into_bytes();

  let user_x_token = login_with_password(&app.state, email, password)
    .await?
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
  let (body, user_x_token) = {
    let request = serde_json::json!({
      "_owner": BASE64_URL_SAFE.encode(setup.user_x),
      "room": BASE64_URL_SAFE.encode(setup.room),
      "data": "user_x message to room",
    });

    let body = serde_json::to_vec(&request).unwrap();

    (body, setup.user_x_token.clone())
  };

  b.to_async(runtime).iter_custom(async |iters| {
    let start = Instant::now();

    let tasks = (0..iters).map(|_i| {
      let body = body.clone();
      let auth = format!("Bearer {user_x_token}");
      let mut router = setup.app.main_router.1.clone();

      return runtime.spawn(async move {
        let response = router
          .call(
            Request::builder()
              .method(http::Method::POST)
              .uri(&format!("/{RECORD_API_PATH}/messages_api"))
              .header(http::header::CONTENT_TYPE, "application/json")
              .header(http::header::AUTHORIZATION, &auth)
              .body(Body::from(body))
              .unwrap(),
          )
          .await
          .unwrap();

        if response.status() != http::StatusCode::OK {
          panic!("Got non-Ok response");
        }
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn benchmark_group(c: &mut Criterion) {
  let runtime = tokio::runtime::Builder::new_multi_thread()
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

criterion_group!(benches, benchmark_group);
criterion_main!(benches);
