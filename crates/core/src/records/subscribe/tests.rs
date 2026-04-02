use async_channel::TryRecvError;
use axum::extract::{Path, RawQuery, State};
use futures_util::StreamExt;
use serde_json::Value;
use trailbase_sqlite::params;

use crate::User;
use crate::admin::user::*;
use crate::app_state::{AppState, test_state};
use crate::auth::util::login_with_password;
use crate::config::proto::RecordApiConfig;
use crate::records::subscribe::event::{JsonEventPayload, deserialize_event};
use crate::records::subscribe::handler::{SubscriptionQuery, add_subscription_sse_and_ws_handler};
use crate::records::test_utils::add_record_api_config;
use crate::records::{PermissionFlag, RecordError};
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
  let stream = manager
    .add_sse_record_subscription(api, trailbase_sqlite::Value::Integer(0), None)
    .await
    .unwrap();

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
    deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap(),
    JsonEventPayload::Ping
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
  match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
    JsonEventPayload::Update { value: obj } => {
      assert_eq!(Value::Object(obj), expected);
    }
    x => {
      panic!("Expected update, got: {x:?}");
    }
  };

  conn
    .execute("DELETE FROM test WHERE _rowid_ = $1", params!(rowid))
    .await
    .unwrap();

  match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
    JsonEventPayload::Delete { value: obj } => {
      assert_eq!(Value::Object(obj), expected);
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

#[tokio::test]
async fn subscribe_to_table_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();

  {
    let stream = manager
      .add_sse_table_subscription(api, None, None)
      .await
      .unwrap();

    assert_eq!(1, manager.num_table_subscriptions());
    // First event is "connection established".
    assert!(matches!(
      deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap(),
      JsonEventPayload::Ping
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

    match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
      JsonEventPayload::Insert { value: obj } => {
        let expected = serde_json::json!({
          "id": record_id_raw,
          "text": "foo",
        });
        assert_eq!(Value::Object(obj), expected);
      }
      x => {
        panic!("Expected insert, got: {x:?}");
      }
    };

    let expected = serde_json::json!({
      "id": record_id_raw,
      "text": "bar",
    });
    match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
      JsonEventPayload::Update { value: obj } => {
        assert_eq!(Value::Object(obj), expected);
      }
      x => {
        panic!("Expected update, got: {x:?}");
      }
    };

    conn
      .execute("DELETE FROM test WHERE id = $1", params!(record_id_raw))
      .await
      .unwrap();

    match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
      JsonEventPayload::Delete { value: obj } => {
        assert_eq!(Value::Object(obj), expected);
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
    let user_x_subscription = manager
      .add_sse_table_subscription(
        api.clone(),
        User::from_auth_token(&state, &user_x_token.auth_token),
        None,
      )
      .await
      .unwrap();

    // First event is "connection established".
    assert!(matches!(
      deserialize_event(user_x_subscription.receiver.recv().await.unwrap().payload).unwrap(),
      JsonEventPayload::Ping
    ));

    let user_y_subscription = manager
      .add_sse_table_subscription(
        api.clone(),
        User::from_auth_token(&state, &user_y_token.auth_token),
        None,
      )
      .await
      .unwrap();

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

    match deserialize_event(user_x_subscription.receiver.recv().await.unwrap().payload).unwrap() {
      JsonEventPayload::Insert { value: obj } => {
        let expected = serde_json::json!({
          "id": record_id_raw,
          "user": uuid_to_b64(&user_x),
          "text": "foo",
        });
        assert_eq!(Value::Object(obj), expected);
      }
      x => {
        panic!("Expected insert, got: {x:?}");
      }
    };

    // User y should *not* have received the insert event.
    assert!(
      tokio::time::timeout(
        tokio::time::Duration::from_millis(300),
        user_y_subscription.receiver.clone().count()
      )
      .await
      .is_err()
    );
    assert_eq!(
      user_y_subscription.receiver.try_recv().err().unwrap(),
      TryRecvError::Empty
    );
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
  let stream = manager
    .add_sse_record_subscription(api, trailbase_sqlite::Value::Integer(record_id), user_x)
    .await
    .unwrap();

  assert_eq!(1, manager.num_record_subscriptions());
  // First event is "connection established".
  assert!(matches!(
    deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap(),
    JsonEventPayload::Ping
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

  match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
    JsonEventPayload::Update { value: obj } => {
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

  match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
    JsonEventPayload::Error { .. } => {}
    x => {
      panic!("Expected error, got: {x:?}");
    }
  }

  conn
    .read_query_row_f("SELECT 1", (), |row| row.get::<_, i64>(0))
    .await
    .unwrap();

  // Make sure the subscription was cleaned up after the access error.
  assert!(stream.receiver.is_closed());
  assert_eq!(0, manager.num_record_subscriptions());
}

#[tokio::test]
async fn subscription_filter_test() {
  let state = setup_world_readable().await;
  let conn = state.conn().clone();

  let manager = state.subscription_manager();
  let api = state.lookup_record_api("api_name").unwrap();

  {
    let filter =
      SubscriptionQuery::parse("filter[$and][0][id][$gt]=5&filter[$and][1][id][$lt]=100").unwrap();

    let stream = manager
      .add_sse_table_subscription(api, None, Some(filter.filter.unwrap()))
      .await
      .unwrap();

    assert_eq!(1, manager.num_table_subscriptions());
    // First event is "connection established".
    assert!(matches!(
      deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap(),
      JsonEventPayload::Ping
    ));

    // This one should be filter out.
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

    match deserialize_event(stream.receiver.recv().await.unwrap().payload).unwrap() {
      JsonEventPayload::Insert { value: obj } => {
        let expected = serde_json::json!({
          "id": 25,
          "text": "foo",
        });
        assert_eq!(Value::Object(obj), expected);
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
