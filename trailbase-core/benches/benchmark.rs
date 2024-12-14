#![allow(clippy::needless_return)]

use criterion::{criterion_group, criterion_main, Criterion};

use axum::body::Body;
use axum::extract::{Json, State};
use axum::http::{self, Request};
use base64::prelude::*;
use std::sync::{Arc, Mutex};
use tower::{Service, ServiceExt};
use trailbase_core::config::proto::PermissionFlag;
use trailbase_core::records::Acls;
use trailbase_sqlite::params;

use trailbase_core::api::{create_user_handler, login_with_password, CreateUserRequest};
use trailbase_core::constants::RECORD_API_PATH;
use trailbase_core::records::{add_record_api, AccessRules};
use trailbase_core::{DataDir, Server, ServerOptions};

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
    .query_row(
      "INSERT INTO room (name) VALUES ($1) RETURNING id",
      params!(name.to_string()),
    )
    .await?
    .unwrap()
    .get(0)?;

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

struct SetupResult {
  app: Server,

  room: [u8; 16],
  user_x: [u8; 16],
  user_x_token: String,
}

async fn setup_app() -> Result<SetupResult, anyhow::Error> {
  let data_dir = temp_dir::TempDir::new()?;

  let app = Server::init(ServerOptions {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    ..Default::default()
  })
  .await?;

  let state = app.state();
  let conn = state.conn();

  create_chat_message_app_tables(conn).await?;
  state.refresh_table_cache().await?;

  let room = add_room(conn, "room0").await?;
  let password = "Secret!1!!";

  let create_access_rule =
    r#"(SELECT 1 FROM room_members WHERE user = _USER_.id AND room = _REQ_.room)"#;

  add_record_api(
    &state,
    "messages_api",
    "message",
    Acls {
      authenticated: vec![PermissionFlag::Read, PermissionFlag::Create],
      ..Default::default()
    },
    AccessRules {
      create: Some(create_access_rule.to_string()),
      ..Default::default()
    },
  )
  .await?;

  let email = "user_x@bar.com";
  let user_x = create_user_handler(
    State(state.clone()),
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

  let user_x_token = login_with_password(&state, email, password)
    .await?
    .auth_token;

  add_user_to_room(conn, user_x, room).await?;

  return Ok(SetupResult {
    app,
    room,
    user_x,
    user_x_token,
  });
}

fn criterion_benchmark(c: &mut Criterion) {
  let runtime = tokio::runtime::Builder::new_current_thread()
    .build()
    .unwrap();

  let mut setup: Option<SetupResult> = None;
  runtime.block_on(async {
    let result = setup_app().await.unwrap();
    let mut router = result.app.router().clone();

    ServiceExt::<Request<Body>>::ready(&mut router)
      .await
      .unwrap();

    let response = router
      .call(
        Request::builder()
          .method(http::Method::GET)
          .uri("/api/healthcheck")
          .body(Body::from(vec![]))
          .unwrap(),
      )
      .await
      .unwrap();

    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
      .await
      .unwrap();

    assert_eq!(bytes.to_vec(), b"Ok");

    setup = Some(result);
  });

  let setup = Arc::new(Mutex::new(setup.take().unwrap()));

  c.bench_function("iter create message", move |b| {
    let mut bencher = b.to_async(&runtime);

    let setup = setup.clone();

    let (body, user_x_token) = {
      let s = setup.lock().unwrap();
      let request = serde_json::json!({
        "_owner": BASE64_URL_SAFE.encode(s.user_x),
        "room": BASE64_URL_SAFE.encode(s.room),
        "data": "user_x message to room",
      });

      let body = serde_json::to_vec(&request).unwrap();

      (body, s.user_x_token.clone())
    };

    bencher.iter(move || {
      let setup = setup.clone();
      let body = body.clone();
      let auth = format!("Bearer {user_x_token}");

      async move {
        let mut router = setup.lock().unwrap().app.router().clone();
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
          let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
          assert!(false, "{body:?}");
        }
      }
    });
  });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
