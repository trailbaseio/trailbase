#[cfg(test)]
mod tests {
  use serde::{Deserialize, Serialize};
  use trailbase_sqlite::params;

  use crate::AppState;
  use crate::records::params::JsonRow;
  use crate::records::{AccessRules, Acls};

  pub use crate::config::proto::RecordApiConfig;

  // NOTE: Prefer add_record_api_config below.
  pub(crate) async fn add_record_api(
    state: &AppState,
    api_name: &str,
    table_name: &str,
    acls: Acls,
    access_rules: AccessRules,
  ) -> Result<(), crate::config::ConfigError> {
    let mut config = state.get_config();

    config.record_apis.push(RecordApiConfig {
      name: Some(api_name.to_string()),
      table_name: Some(table_name.to_string()),

      acl_world: acls.world.into_iter().map(|f| f as i32).collect(),
      acl_authenticated: acls.authenticated.into_iter().map(|f| f as i32).collect(),
      conflict_resolution: None,
      autofill_missing_user_id_columns: None,
      enable_subscriptions: None,
      excluded_columns: vec![],
      create_access_rule: access_rules.create,
      read_access_rule: access_rules.read,
      update_access_rule: access_rules.update,
      delete_access_rule: access_rules.delete,
      schema_access_rule: access_rules.schema,
      expand: vec![],
      listing_hard_limit: None,
    });

    return state.validate_and_update_config(config, None).await;
  }

  pub(crate) async fn add_record_api_config(
    state: &AppState,
    api: RecordApiConfig,
  ) -> Result<(), crate::config::ConfigError> {
    let mut config = state.get_config();
    config.record_apis.push(api);
    return state.validate_and_update_config(config, None).await;
  }

  #[derive(Debug, Deserialize, Serialize, PartialEq)]
  pub struct Message {
    pub mid: String,
    pub _owner: Option<String>,
    pub room: String,
    pub data: String,
  }

  pub fn to_message(v: serde_json::Value) -> Message {
    return match v {
      serde_json::Value::Object(ref obj) => {
        let mut keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
        keys.sort();
        assert_eq!(keys, ["data", "mid", "room", "table"], "Got: {keys:?}");
        serde_json::from_value::<Message>(v).unwrap()
      }
      _ => panic!("expected object, got {v:?}"),
    };
  }

  pub async fn create_chat_message_app_tables(state: &AppState) -> Result<(), anyhow::Error> {
    // Create a messages, chat room and members tables.
    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE room (
            rid          BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(rid)) DEFAULT(uuid_v7()),
            name         TEXT
          ) STRICT;

          CREATE TABLE message (
            mid          BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(mid)) DEFAULT (uuid_v7()),
            _owner       BLOB NOT NULL,
            room         BLOB NOT NULL,
            data         TEXT NOT NULL DEFAULT 'empty',

            -- Dummy column with a name requiring escaping.
            'table'      INTEGER NOT NULL DEFAULT 0,

            -- on user delete, toombstone it.
            FOREIGN KEY(_owner) REFERENCES _user(id) ON DELETE SET NULL,
            -- On chatroom delete, delete message
            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE
          ) STRICT;

          CREATE TABLE room_members (
            user         BLOB NOT NULL,
            room         BLOB NOT NULL,

            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE,
            FOREIGN KEY(user) REFERENCES _user(id) ON DELETE CASCADE
          ) STRICT;
        "#,
      )
      .await?;

    state.rebuild_schema_cache().await.unwrap();

    return Ok(());
  }

  pub async fn create_chat_message_app_tables_integer(
    state: &AppState,
  ) -> Result<(), anyhow::Error> {
    // Create a messages, chat room and members tables.
    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE room (
            rid          BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(rid)) DEFAULT(uuid_v7()),
            name         TEXT
          ) STRICT;

          CREATE TABLE message (
            mid          INTEGER PRIMARY KEY,
            _owner       BLOB NOT NULL,
            room         BLOB NOT NULL,
            data         TEXT NOT NULL DEFAULT 'empty',

            -- on user delete, toombstone it.
            FOREIGN KEY(_owner) REFERENCES _user(id) ON DELETE SET NULL,
            -- On chatroom delete, delete message
            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE
          ) STRICT;

          CREATE TABLE room_members (
            user         BLOB NOT NULL,
            room         BLOB NOT NULL,

            FOREIGN KEY(room) REFERENCES room(rid) ON DELETE CASCADE,
            FOREIGN KEY(user) REFERENCES _user(id) ON DELETE CASCADE
          ) STRICT;
        "#,
      )
      .await?;

    state.rebuild_schema_cache().await.unwrap();

    return Ok(());
  }

  pub async fn add_room(
    conn: &trailbase_sqlite::Connection,
    name: &str,
  ) -> Result<[u8; 16], anyhow::Error> {
    let room: [u8; 16] = conn
      .query_row_f(
        "INSERT INTO room (name) VALUES ($1) RETURNING rid",
        params!(name.to_string()),
        |row| row.get(0),
      )
      .await?
      .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

    return Ok(room);
  }

  pub async fn add_user_to_room(
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

  pub async fn send_message(
    conn: &trailbase_sqlite::Connection,
    user: [u8; 16],
    room: [u8; 16],
    message: &str,
  ) -> Result<[u8; 16], anyhow::Error> {
    let id: [u8; 16] = conn
      .query_row_f(
        "INSERT INTO message (_owner, room, data) VALUES ($1, $2, $3) RETURNING mid",
        params!(user, room, message.to_string()),
        |row| row.get(0),
      )
      .await?
      .ok_or(rusqlite::Error::QueryReturnedNoRows)?;

    return Ok(id);
  }

  pub fn json_row_from_value(value: serde_json::Value) -> Result<JsonRow, anyhow::Error> {
    return match value {
      serde_json::Value::Object(map) => Ok(map),
      _ => Err(anyhow::anyhow!("Not an object: {value:?}")),
    };
  }
}

#[cfg(test)]
pub(crate) use tests::*;
