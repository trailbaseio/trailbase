use axum::extract::{Path, RawQuery, State};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::time::timeout;
use trailbase_sqlite::params;

use crate::User;
use crate::admin::user::*;
use crate::app_state::{AppState, test_state};
use crate::auth::util::login_with_password;
use crate::config::proto::RecordApiConfig;
use crate::records::subscribe::event::{TestChangeEvent, TestJsonEventPayload};
use crate::records::subscribe::handler::{
  SubscriptionQuery, add_subscription_sse_and_ws_handler, subscribe_sse,
};
use crate::records::test_utils::add_record_api_config;
use crate::records::{PermissionFlag, RecordApi, RecordError};
use crate::util::uuid_to_b64;

async fn setup_world_readable() -> AppState {
  let state = test_state(None).await.unwrap();
  let conn = state.conn().clone();

  conn
    .execute(
      "CREATE TABLE test (id INTEGER PRIMARY KEY, text TEXT) STRICT",
      (),
    )
    .await
    .unwrap();

  state.rebuild_connection_metadata().await.unwrap();

  // Register message table as record api with moderator read access.
  add_record_api_config(
    &state,
    RecordApiConfig {
      name: Some("api_name".to_string()),
      table_name: Some("test".to_string()),
      enable_subscriptions: Some(true),
      acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
      ..Default::default()
    },
  )
  .await
  .unwrap();

  return state;
}

#[tokio::test]
async fn subscribe_to_record_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let record_id_raw = 0;
  let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
  let rowid: i64 = conn
    .query_row_f(
      "INSERT INTO test (id, text) VALUES ($1, 'foo') RETURNING _rowid_",
      [record_id],
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  assert_eq!(rowid, record_id_raw);

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();
  // let stream = manager
  //   .add_sse_record_subscription(api, trailbase_sqlite::Value::Integer(0), None)
  //   .await
  //   .unwrap();

  let mut stream = subscribe_to_records(state.clone(), api, "0", None, /* filter= */ None).await;

  assert_eq!(1, manager.num_record_subscriptions());

  // Make sure rebuilding connection metadata doesn't drop subscriptions.
  state.rebuild_connection_metadata().await.unwrap();

  assert_eq!(1, manager.num_record_subscriptions());

  // Make sure updating config doesn't drop subscriptions.
  state
    .validate_and_update_config(state.get_config(), None)
    .await
    .unwrap();

  // First event is "connection established".
  assert!(matches!(
    stream.next().await.unwrap().event,
    TestJsonEventPayload::Ping
  ));

  // This should do nothing since nobody is subscribed to id = 5.
  let _ = conn
    .execute(
      "INSERT INTO test (id, text) VALUES ($1, 'baz')",
      [trailbase_sqlite::Value::Integer(5)],
    )
    .await
    .unwrap();

  conn
    .execute(
      "UPDATE test SET text = $1 WHERE _rowid_ = $2",
      params!("bar", rowid),
    )
    .await
    .unwrap();

  let expected = serde_json::json!({
    "id": record_id_raw,
    "text": "bar",
  });
  match stream.next().await.unwrap().event {
    TestJsonEventPayload::Update(obj) => {
      assert_eq!(Value::Object(obj.clone()), expected);
    }
    x => {
      panic!("Expected update, got: {x:?}");
    }
  };

  conn
    .execute("DELETE FROM test WHERE _rowid_ = $1", params!(rowid))
    .await
    .unwrap();

  match stream.next().await.unwrap().event {
    TestJsonEventPayload::Delete(obj) => {
      assert_eq!(Value::Object(obj.clone()), expected);
    }
    x => {
      panic!("Expected delete, got: {x:?}");
    }
  }

  // Implicitly await for scheduled cleanups to go through.
  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();
}

async fn subscribe_to_records(
  state: AppState,
  api: RecordApi,
  record: &str,
  user: Option<User>,
  filter: Option<&str>,
  // ) -> kanal::AsyncReceiver<TestChangeEvent> {
) -> std::pin::Pin<Box<dyn futures_util::Stream<Item = TestChangeEvent>>> {
  let filter = filter.map(|f| SubscriptionQuery::parse(f).unwrap().filter.unwrap());
  let response = subscribe_sse(state, api, record.to_string(), filter, user)
    .await
    .unwrap();

  assert_eq!(200, response.status());

  let cnt = Arc::new(AtomicI64::default());
  let stream = response.into_data_stream();

  return stream
    .take_while(|bytes| std::future::ready(bytes.is_ok()))
    .filter_map(move |bytes| {
      let cnt = cnt.clone();

      return async move {
        let Ok(bytes) = bytes.as_ref() else {
          return None;
        };
        let payload = String::from_utf8_lossy(&bytes).to_string();

        cnt.fetch_add(1, Ordering::SeqCst);
        if cnt.load(Ordering::SeqCst) == 1 {
          // Make sure we have an explicit ping as a first message to establish connection.
          assert!(payload.contains("ping"));

          return Some(TestChangeEvent {
            event: TestJsonEventPayload::Ping,
            seq: None,
          });
        }

        // Ignore heartbeats.
        if let Some((_a, b)) = payload.split_once("data: ") {
          return Some(serde_json::from_str(b).unwrap());
        }
        return None;
      };
    })
    .boxed();
}

async fn take_test_events(
  stream: std::pin::Pin<Box<dyn futures_util::Stream<Item = TestChangeEvent>>>,
  n: usize,
) -> Vec<TestChangeEvent> {
  use tokio::time::*;

  return timeout(Duration::from_secs(4), stream.take(n).collect())
    .await
    .unwrap();
}

#[tokio::test]
async fn subscribe_to_table_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();

  {
    // let stream = manager
    //   .add_sse_table_subscription(api, None, None)
    //   .await
    //   .unwrap();

    let mut stream = subscribe_to_records(state.clone(), api, "*", None, /* filter= */ None).await;

    assert_eq!(1, manager.num_table_subscriptions());
    // First event is "connection established".
    assert!(matches!(
      stream.next().await.unwrap().event,
      TestJsonEventPayload::Ping
    ));

    let record_id_raw = 0;
    conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, 'foo')",
        params!(record_id_raw),
      )
      .await
      .unwrap();

    conn
      .execute(
        "UPDATE test SET text = $1 WHERE id = $2",
        params!("bar", record_id_raw),
      )
      .await
      .unwrap();

    match stream.next().await.unwrap().event {
      TestJsonEventPayload::Insert(obj) => {
        let expected = serde_json::json!({
          "id": record_id_raw,
          "text": "foo",
        });
        assert_eq!(Value::Object(obj.clone()), expected);
      }
      x => {
        panic!("Expected insert, got: {x:?}");
      }
    };

    let expected = serde_json::json!({
      "id": record_id_raw,
      "text": "bar",
    });
    match stream.next().await.unwrap().event {
      TestJsonEventPayload::Update(obj) => {
        assert_eq!(Value::Object(obj.clone()), expected);
      }
      x => {
        panic!("Expected update, got: {x:?}");
      }
    };

    conn
      .execute("DELETE FROM test WHERE id = $1", params!(record_id_raw))
      .await
      .unwrap();

    match stream.next().await.unwrap().event {
      TestJsonEventPayload::Delete(obj) => {
        assert_eq!(Value::Object(obj.clone()), expected);
      }
      x => {
        panic!("Expected delete, got: {x:?}");
      }
    }
  }

  // Implicitly await for scheduled cleanups to go through.
  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();

  assert_eq!(0, manager.num_table_subscriptions());
}

#[tokio::test]
async fn subscription_lifecycle_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let record_id_raw = 0;
  let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
  let rowid: i64 = conn
    .query_row_f(
      "INSERT INTO test (id, text) VALUES ($1, 'foo') RETURNING _rowid_",
      [record_id],
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  assert_eq!(rowid, record_id_raw);

  let sse = add_subscription_sse_and_ws_handler(
    State(state.clone()),
    Path(("api_name".to_string(), record_id_raw.to_string())),
    None,
    RawQuery(None),
    axum::extract::Request::default(),
  )
  .await;

  let manager = state.subscription_manager();
  assert_eq!(1, manager.num_record_subscriptions());

  drop(sse);

  // Implicitly await for the cleanup to be scheduled on the sqlite executor.
  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();

  assert_eq!(0, manager.num_record_subscriptions());
}

async fn setup_with_tight_acls() -> AppState {
  let state = test_state(None).await.unwrap();
  let conn = state.conn().clone();

  conn
    .execute(
      "CREATE TABLE test (
            id          INTEGER PRIMARY KEY,
            user        BLOB NOT NULL,
            text        TEXT
         ) STRICT",
      (),
    )
    .await
    .unwrap();

  state.rebuild_connection_metadata().await.unwrap();

  // Register message table as record api with moderator read access.
  add_record_api_config(
    &state,
    RecordApiConfig {
      name: Some("api_name".to_string()),
      table_name: Some("test".to_string()),
      enable_subscriptions: Some(true),
      acl_authenticated: [PermissionFlag::Read as i32].into(),
      read_access_rule: Some(
        "EXISTS(SELECT 1 FROM test AS m WHERE _USER_.id = _ROW_.user)".to_string(),
      ),
      ..Default::default()
    },
  )
  .await
  .unwrap();

  return state;
}

#[tokio::test]
async fn subscription_acl_test() {
  let state = setup_with_tight_acls().await;
  let conn = state.conn();

  let user_x_email = "user_x@bar.com";
  let password = "Secret!1!!";

  let sse_or = add_subscription_sse_and_ws_handler(
    State(state.clone()),
    Path(("api_name".to_string(), "*".to_string())),
    None,
    RawQuery(None),
    axum::extract::Request::default(),
  )
  .await;

  assert!(matches!(sse_or, Err(RecordError::Forbidden)));

  let user_x = create_user_for_test(&state, user_x_email, password)
    .await
    .unwrap()
    .into_bytes();
  let user_x_token = login_with_password(&state, user_x_email, password)
    .await
    .unwrap();

  // Check that we can subscribe to table wide changes.
  {
    let _ = add_subscription_sse_and_ws_handler(
      State(state.clone()),
      Path(("api_name".to_string(), "*".to_string())),
      User::from_auth_token(&state, &user_x_token.auth_token),
      RawQuery(None),
      axum::extract::Request::default(),
    )
    .await
    .unwrap();
  }

  let record_id_raw = 0;
  let record_id = trailbase_sqlite::Value::Integer(record_id_raw);
  let _rowid: i64 = conn
    .query_row_f(
      "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo') RETURNING _rowid_",
      [
        record_id.clone(),
        trailbase_sqlite::Value::Blob(user_x.to_vec()),
      ],
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  // Assert user_x can subscribe to their record.
  {
    let _ = add_subscription_sse_and_ws_handler(
      State(state.clone()),
      Path(("api_name".to_string(), record_id_raw.to_string())),
      User::from_auth_token(&state, &user_x_token.auth_token),
      RawQuery(None),
      axum::extract::Request::default(),
    )
    .await
    .unwrap();
  }

  // Assert user_y cannot subscribe to user_x's record.
  {
    let user_y_email = "user_y@bar.com";
    let _user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    let sse_or = add_subscription_sse_and_ws_handler(
      State(state.clone()),
      Path(("api_name".to_string(), record_id_raw.to_string())),
      User::from_auth_token(&state, &user_y_token.auth_token),
      RawQuery(None),
      axum::extract::Request::default(),
    )
    .await;

    assert!(matches!(sse_or, Err(RecordError::Forbidden)));
  }
}

#[tokio::test]
async fn test_acl_selective_table_subs() {
  let state = setup_with_tight_acls().await;
  let conn = state.conn();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();

  let password = "Secret!1!!";
  let user_x_email = "user_x@bar.com";
  let user_x = create_user_for_test(&state, user_x_email, password)
    .await
    .unwrap();
  let user_x_token = login_with_password(&state, user_x_email, password)
    .await
    .unwrap();

  let user_y_email = "user_y@bar.com";
  let _user_y = create_user_for_test(&state, user_y_email, password)
    .await
    .unwrap()
    .into_bytes();
  let user_y_token = login_with_password(&state, user_y_email, password)
    .await
    .unwrap();

  // Assert events for table subscriptions are selective on ACLs.
  {
    let mut user_x_subscription = subscribe_to_records(
      state.clone(),
      api.clone(),
      "*",
      User::from_auth_token(&state, &user_x_token.auth_token),
      /* filter= */ None,
    )
    .await;
    // let user_x_subscription = manager
    //   .add_sse_table_subscription(
    //     api.clone(),
    //     User::from_auth_token(&state, &user_x_token.auth_token),
    //     None,
    //   )
    //   .await
    //   .unwrap();

    // First event is "connection established".
    assert!(matches!(
      user_x_subscription.next().await.unwrap().event,
      TestJsonEventPayload::Ping
    ));

    // let user_y_subscription = manager
    //   .add_sse_table_subscription(
    //     api.clone(),
    //     User::from_auth_token(&state, &user_y_token.auth_token),
    //     None,
    //   )
    //   .await
    //   .unwrap();

    let mut user_y_subscription = subscribe_to_records(
      state.clone(),
      api.clone(),
      "*",
      User::from_auth_token(&state, &user_y_token.auth_token),
      /* filter= */ None,
    )
    .await;

    assert_eq!(2, manager.num_table_subscriptions());

    let record_id_raw = 1;
    conn
      .execute(
        "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo')",
        [
          trailbase_sqlite::Value::Integer(record_id_raw),
          trailbase_sqlite::Value::Blob(user_x.into()),
        ],
      )
      .await
      .unwrap();

    match user_x_subscription.next().await.unwrap().event {
      TestJsonEventPayload::Insert(obj) => {
        let expected = serde_json::json!({
          "id": record_id_raw,
          "user": uuid_to_b64(&user_x),
          "text": "foo",
        });
        assert_eq!(Value::Object(obj.clone()), expected);
      }
      x => {
        panic!("Expected insert, got: {x:?}");
      }
    };

    // User y should *not* have received the insert event.
    // assert!(
    //   tokio::time::timeout(
    //     tokio::time::Duration::from_millis(300),
    //     user_y_subscription.receiver.clone().count()
    //   )
    //   .await
    //   .is_err()
    // );
    // assert_eq!(
    //   user_y_subscription.next().await.unwrap(),
    //   TryRecvError::Empty
    // );
    assert!(matches!(
      user_y_subscription.next().await.unwrap().event,
      TestJsonEventPayload::Ping
    ));

    use tokio::time::*;
    let got = timeout(Duration::from_millis(100), user_y_subscription.next()).await;
    assert!(got.is_err(), "Got: {got:?}");
  }

  // Implicitly await for scheduled cleanups to go through.
  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();

  assert_eq!(0, manager.num_table_subscriptions());
}

#[tokio::test]
async fn subscription_acl_change_owner() {
  let state = setup_with_tight_acls().await;
  let conn = state.conn();

  let user_x_email = "user_x@bar.com";
  let password = "Secret!1!!";

  let user_x_id = create_user_for_test(&state, user_x_email, password)
    .await
    .unwrap();
  let user_x_token = login_with_password(&state, user_x_email, password)
    .await
    .unwrap();
  let user_x = User::from_auth_token(&state, &user_x_token.auth_token);

  let record_id = 0;
  let _rowid: i64 = conn
    .query_row_f(
      "INSERT INTO test (id, user, text) VALUES ($1, $2, 'foo') RETURNING _rowid_",
      [
        trailbase_sqlite::Value::Integer(record_id),
        trailbase_sqlite::Value::Blob(user_x_id.into()),
      ],
      |row| row.get(0),
    )
    .await
    .unwrap()
    .unwrap();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();
  let mut stream = subscribe_to_records(
    state.clone(),
    api,
    &record_id.to_string(),
    user_x,
    /* filter= */ None,
  )
  .await;
  // manager
  // .add_sse_record_subscription(api, trailbase_sqlite::Value::Integer(record_id), user_x)
  // .await
  // .unwrap();

  assert_eq!(1, manager.num_record_subscriptions());
  // First event is "connection established".
  assert!(matches!(
    stream.next().await.unwrap().event,
    TestJsonEventPayload::Ping,
  ));

  conn
    .execute(
      "UPDATE test SET text = $1 WHERE id = $2",
      params!("bar", record_id),
    )
    .await
    .unwrap();

  // Unset the owner. This Update should no longer be delivered.
  conn
    .execute(
      "UPDATE test SET user = $1 WHERE id = $2",
      params!(Vec::<u8>::new(), record_id),
    )
    .await
    .unwrap();

  match stream.next().await.unwrap().event {
    TestJsonEventPayload::Update(obj) => {
      let expected = serde_json::json!({
        "id": record_id,
        "user": uuid_to_b64(&user_x_id),
        "text": "bar",
      });
      assert_eq!(Value::Object(obj), expected);
    }
    x => {
      panic!("Expected update, got: {x:?}");
    }
  }

  match stream.next().await.unwrap().event {
    TestJsonEventPayload::Error(_) => {}
    x => {
      panic!("Expected error, got: {x:?}");
    }
  }

  drop(stream);

  // Make sure the subscription was cleaned up after the access error.
  // assert!(stream.is_closed());
  assert_eq!(0, manager.num_record_subscriptions());
}

#[tokio::test]
async fn subscription_filter_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();

  {
    let stream = subscribe_to_records(
      state.clone(),
      api.clone(),
      "*",
      /* user= */ None,
      Some("filter[$and][0][id][$gt]=5&filter[$and][1][id][$lt]=100"),
    )
    .await;

    assert_eq!(1, manager.num_table_subscriptions());

    conn
      .execute("INSERT INTO test (id, text) VALUES ($1, 'foo')", params!(1))
      .await
      .unwrap();

    // This one should get through.
    conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, 'foo')",
        params!(25),
      )
      .await
      .unwrap();

    let events = take_test_events(stream, 2).await;

    assert!(matches!(events[0].event, TestJsonEventPayload::Ping));

    match &events[1].event {
      TestJsonEventPayload::Insert(obj) => {
        let expected = serde_json::json!({
          "id": 25,
          "text": "foo",
        });
        assert_eq!(Value::Object(obj.clone()), expected);
      }
      x => {
        panic!("Expected insert, got: {x:?}");
      }
    };
  }

  // Implicitly await for scheduled cleanups to go through.
  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();

  assert_eq!(0, manager.num_table_subscriptions());
}
