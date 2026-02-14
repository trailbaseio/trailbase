#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]

use parking_lot::RwLock;
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

pub mod geoip;
pub mod jsonschema;
pub mod password;

mod base64;
mod regex;
mod uuid;
mod validators;

use crate::jsonschema::JsonSchemaRegistry;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("Other error: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

pub fn apply_default_pragmas(conn: &Connection) -> Result<(), rusqlite::Error> {
  conn.pragma_update(None, "busy_timeout", 10000)?;
  conn.pragma_update(None, "journal_mode", "WAL")?;
  conn.pragma_update(None, "journal_size_limit", 200000000)?;
  // Sync the file system less often.
  conn.pragma_update(None, "synchronous", "NORMAL")?;
  conn.pragma_update(None, "foreign_keys", "ON")?;
  conn.pragma_update(None, "temp_store", "MEMORY")?;
  // TODO: we could consider pushing this further down-stream to optimize
  // for different use-cases, e.g. main vs logs.
  conn.pragma_update(None, "cache_size", -16000)?;
  // Keep SQLite default 4KB page_size
  // conn.pragma_update(None, "page_size", 4096)?;

  // Safety feature around application-defined functions recommended by
  // https://sqlite.org/appfunc.html
  conn.pragma_update(None, "trusted_schema", "OFF")?;

  // Make like operator case-sensitive. It's the default for Postgres and users certainly would not
  // expect that `filter[col][$like]=%foo%` finds "FoO".
  conn.pragma_update(None, "case_sensitive_like", "ON")?;

  return Ok(());
}

pub fn connect_sqlite(
  path: Option<PathBuf>,
  registry: Option<Arc<RwLock<JsonSchemaRegistry>>>,
) -> Result<Connection, Error> {
  // NOTE: We used to initialize C extensions here as well, such as sqlean and sqlite-vec, however
  // this has now been moved to the top-level CLI.

  // Then open database and load trailbase_extensions.
  let conn = if let Some(p) = path {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
      | OpenFlags::SQLITE_OPEN_CREATE
      | OpenFlags::SQLITE_OPEN_NO_MUTEX;

    Connection::open_with_flags(p, flags)?
  } else {
    Connection::open_in_memory()?
  };

  register_all_extension_functions(&conn, registry)?;

  apply_default_pragmas(&conn)?;

  // Initial optimize.
  conn.pragma_update(None, "optimize", "0x10002")?;

  return Ok(conn);
}

pub fn register_all_extension_functions(
  db: &Connection,
  registry: Option<Arc<RwLock<JsonSchemaRegistry>>>,
) -> Result<(), rusqlite::Error> {
  // WARN: Be careful with declaring INNOCUOUS. It allows "user-defined functions" to run
  // when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  uuid::register_extension_functions(db)?;
  password::register_extension_functions(db)?;
  jsonschema::register_extension_functions(db, registry)?;
  geoip::register_extension_functions(db)?;
  base64::register_extension_functions(db)?;
  regex::register_extension_functions(db)?;
  validators::register_extension_functions(db)?;

  return Ok(());
}

#[cfg(test)]
mod test {
  use ::uuid::Uuid;
  use rusqlite::Error;

  use super::*;

  #[test]
  fn test_connect_and_extensions() {
    let conn = connect_sqlite(None, None).unwrap();

    let row = conn
      .query_row("SELECT (uuid_v7())", (), |row| -> Result<[u8; 16], Error> {
        row.get(0)
      })
      .unwrap();

    let uuid = Uuid::from_bytes(row);
    assert_eq!(uuid.get_version_num(), 7);
  }

  #[test]
  fn test_uuids() {
    let conn = connect_sqlite(None, None).unwrap();

    conn
      .execute(
        r#"CREATE TABLE test (
        id    BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
        text  TEXT
      )"#,
        (),
      )
      .unwrap();

    // V4 fails
    assert!(
      conn
        .execute(
          "INSERT INTO test (id) VALUES (?1) ",
          rusqlite::params!(Uuid::new_v4().into_bytes())
        )
        .is_err()
    );

    // V7 succeeds
    let id = Uuid::now_v7();
    assert!(
      conn
        .execute(
          "INSERT INTO test (id) VALUES (?1) ",
          rusqlite::params!(id.into_bytes())
        )
        .is_ok()
    );

    let read_id: Uuid = conn
      .query_row("SELECT id FROM test LIMIT 1", [], |row| {
        Ok(Uuid::from_bytes(row.get::<_, [u8; 16]>(0)?))
      })
      .unwrap();

    assert_eq!(id, read_id);
  }
}
