#[cfg(test)]
mod tests {
  use trailbase_sqlite::params;

  use crate::records::params::JsonRow;
  use crate::AppState;

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

    state.table_metadata().invalidate_all().await.unwrap();

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

    state.table_metadata().invalidate_all().await.unwrap();

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
