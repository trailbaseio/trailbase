#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]

use rusqlite::functions::FunctionFlags;
use std::path::PathBuf;

pub mod geoip;
pub mod jsonschema;
pub mod password;

mod b64;
mod regex;
mod uuid;
mod validators;

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),
  #[error("Other error: {0}")]
  Other(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

pub fn apply_default_pragmas(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
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

  return Ok(());
}

#[allow(unsafe_code)]
pub fn connect_sqlite(path: Option<PathBuf>) -> Result<rusqlite::Connection, Error> {
  // First load C extensions like sqlean and vector search.
  let status =
    unsafe { rusqlite::ffi::sqlite3_auto_extension(Some(init_sqlean_and_vector_search)) };
  if status != 0 {
    return Err(Error::Other("Failed to load extensions".into()));
  }

  // Then open database and load trailbase_extensions.
  let conn = sqlite3_extension_init(if let Some(p) = path {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
      | OpenFlags::SQLITE_OPEN_CREATE
      | OpenFlags::SQLITE_OPEN_NO_MUTEX;

    rusqlite::Connection::open_with_flags(p, flags)?
  } else {
    rusqlite::Connection::open_in_memory()?
  })?;

  apply_default_pragmas(&conn)?;

  // Initial optimize.
  conn.pragma_update(None, "optimize", "0x10002")?;

  return Ok(conn);
}

pub fn sqlite3_extension_init(
  db: rusqlite::Connection,
) -> Result<rusqlite::Connection, rusqlite::Error> {
  // WARN: Be careful with declaring INNOCUOUS. This allows these "app-defined functions" to run
  // even when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  db.create_scalar_function(
    "is_uuid",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    uuid::is_uuid,
  )?;
  db.create_scalar_function(
    "is_uuid_v4",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    uuid::is_uuid_v4,
  )?;
  db.create_scalar_function("uuid_v4", 0, FunctionFlags::SQLITE_INNOCUOUS, uuid::uuid_v4)?;
  db.create_scalar_function(
    "is_uuid_v7",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    uuid::is_uuid_v7,
  )?;
  db.create_scalar_function("uuid_v7", 0, FunctionFlags::SQLITE_INNOCUOUS, uuid::uuid_v7)?;
  db.create_scalar_function(
    "uuid_text",
    1,
    FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
    uuid::uuid_text,
  )?;

  db.create_scalar_function(
    "uuid_parse",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    uuid::uuid_parse,
  )?;

  // Used to create initial user credentials in migrations.
  db.create_scalar_function(
    "hash_password",
    1,
    FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
    password::hash_password_sqlite,
  )?;

  // Match column against given JSON schema, e.g. jsonschema_matches(col, '<schema>').
  db.create_scalar_function(
    "jsonschema_matches",
    2,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    jsonschema::jsonschema_matches,
  )?;
  // Match column against registered JSON schema by name, e.g. jsonschema(col, 'schema-name').
  db.create_scalar_function(
    "jsonschema",
    2,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    jsonschema::jsonschema_by_name,
  )?;
  db.create_scalar_function(
    "jsonschema",
    3,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    jsonschema::jsonschema_by_name_with_extra_args,
  )?;

  // Validators for CHECK constraints.
  db.create_scalar_function(
    // NOTE: the name needs to be "regexp" to be picked up by sqlites REGEXP matcher:
    // https://www.sqlite.org/lang_expr.html
    "regexp",
    2,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    regex::regexp,
  )?;
  db.create_scalar_function(
    "is_email",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    validators::is_email,
  )?;
  // NOTE: there's also https://sqlite.org/json1.html#jvalid
  db.create_scalar_function(
    "is_json",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    validators::is_json,
  )?;

  db.create_scalar_function(
    "geoip_country",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip::geoip_country,
  )?;
  db.create_scalar_function(
    "geoip_city_name",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip::geoip_city_name,
  )?;
  db.create_scalar_function(
    "geoip_city_json",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip::geoip_city_json,
  )?;

  db.create_scalar_function(
    "base64_url_safe",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    b64::base64_url_safe,
  )?;

  return Ok(db);
}

#[allow(unsafe_code)]
#[unsafe(no_mangle)]
extern "C" fn init_sqlean_and_vector_search(
  db: *mut rusqlite::ffi::sqlite3,
  _pz_err_msg: *mut *mut std::os::raw::c_char,
  _p_api: *const rusqlite::ffi::sqlite3_api_routines,
) -> ::std::os::raw::c_int {
  // Add sqlite-vec extension.
  unsafe {
    sqlite_vec::sqlite3_vec_init();
  }

  // Init sqlean's stored procedures: "define", see:
  //   https://github.com/nalgeon/sqlean/blob/main/docs/define.md
  let status = unsafe { trailbase_sqlean::define_init(db as *mut trailbase_sqlean::sqlite3) };
  if status != 0 {
    log::error!("Failed to load sqlean::define",);
    return status;
  }

  return status;
}

#[cfg(test)]
mod test {
  use ::uuid::Uuid;
  use rusqlite::Error;

  use super::*;

  #[test]
  fn test_connect_and_extensions() {
    let conn = connect_sqlite(None).unwrap();

    let row = conn
      .query_row("SELECT (uuid_v7())", (), |row| -> Result<[u8; 16], Error> {
        row.get(0)
      })
      .unwrap();

    let uuid = Uuid::from_bytes(row);
    assert_eq!(uuid.get_version_num(), 7);

    // sqlean: Define a stored procedure, use it, and remove it.
    conn
      .query_row("SELECT define('sumn', ':n * (:n + 1) / 2')", (), |_row| {
        Ok(())
      })
      .unwrap();

    let value: i64 = conn
      .query_row("SELECT sumn(5)", (), |row| row.get(0))
      .unwrap();
    assert_eq!(value, 15);

    conn
      .query_row("SELECT undefine('sumn')", (), |_row| Ok(()))
      .unwrap();

    // sqlite-vec
    conn
      .query_row("SELECT vec_f32('[0, 1, 2, 3]')", (), |_row| Ok(()))
      .unwrap();
  }

  #[test]
  fn test_uuids() {
    let conn = connect_sqlite(None).unwrap();

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
