use axum::{
  extract::{
    ws::{Message, WebSocket, WebSocketUpgrade},
    Path, State,
  },
  response::Response,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::auth::user::User;
use crate::records::{Permission, RecordError};
use crate::AppState;

pub struct Subscription {
  record: Option<trailbase_sqlite::Value>,
  user: Option<User>,
  channel: mpsc::Sender<serde_json::Value>,
}

pub struct SubscriptionManager {
  conn: trailbase_sqlite::Connection,

  /// Map from table name to subscriptions.
  ///
  /// TODO: Make this work with a RecordId trait to allow for different record ids.
  subscriptions: Arc<RwLock<HashMap<String, HashMap<trailbase_sqlite::Value, Subscription>>>>,
}

impl SubscriptionManager {
  pub fn new(conn: trailbase_sqlite::Connection) -> Result<Self, crate::InitError> {
    let subscriptions = Arc::new(RwLock::new(HashMap::new()));

    return Ok(Self {
      conn,
      subscriptions,
    });
  }

  async fn add_hook(&self) {
    let subscriptions = self.subscriptions.clone();
    self
      .conn
      .call(|conn| {
        conn.update_hook(Some(
          move |_action: rusqlite::hooks::Action, _db_name: &str, table_name: &str, _rowid: i64| {
            if let Some(_subs) = subscriptions.read().get(table_name) {}
          },
        ));

        return Ok(());
      })
      .await
      .unwrap();
  }

  fn add_subscription(
    &self,
    table: String,
    record: Option<trailbase_sqlite::Value>,
    user: Option<User>,
  ) -> Result<mpsc::Receiver<serde_json::Value>, RecordError> {
    let Some(record) = record else {
      return Err(RecordError::BadRequest("Missing record id"));
    };
    let (sender, receiver) = mpsc::channel::<serde_json::Value>(16);

    let m: &mut HashMap<trailbase_sqlite::Value, Subscription> =
      self.subscriptions.write().entry(table).or_default();

    // m.insert(
    //   record.clone(),
    //   Subscription {
    //     record: Some(record),
    //     user,
    //     channel: sender,
    //   },
    // );

    return Ok(receiver);
  }
}

pub async fn handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
  ws: WebSocketUpgrade,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.id_to_sql(&record)?;

  let Ok(()) = api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await
  else {
    return Err(RecordError::Forbidden);
  };

  let receiver = state.subscription_manager().add_subscription(
    api.table_name().to_string(),
    Some(record_id),
    user,
  )?;

  return Ok(ws.on_upgrade(move |socket| handle_socket(socket, receiver)));
}

async fn handle_socket(mut socket: WebSocket, mut receiver: mpsc::Receiver<serde_json::Value>) {
  while let Some(msg) = receiver.recv().await {
    // let msg = if let Ok(msg) = msg {
    //   msg
    // } else {
    //   // client disconnected
    //   return;
    // };

    if socket.send(Message::Text(msg.to_string())).await.is_err() {
      // client disconnected
      return;
    }
  }
}
