use crate::Error;
use std::path::PathBuf;

#[allow(unsafe_code)]
#[no_mangle]
extern "C" fn init_trailbase_extensions(
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

#[allow(unsafe_code)]
pub fn connect_sqlite(
  path: Option<PathBuf>,
  extensions: Option<Vec<PathBuf>>,
) -> Result<rusqlite::Connection, Error> {
  crate::schema::try_init_schemas();

  let status = unsafe { rusqlite::ffi::sqlite3_auto_extension(Some(init_trailbase_extensions)) };
  if status != 0 {
    return Err(Error::Other("Failed to load extensions".into()));
  }

  let conn = trailbase_extension::sqlite3_extension_init(if let Some(p) = path {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
      | OpenFlags::SQLITE_OPEN_CREATE
      | OpenFlags::SQLITE_OPEN_NO_MUTEX;

    rusqlite::Connection::open_with_flags(p, flags)?
  } else {
    rusqlite::Connection::open_in_memory()?
  })?;
  conn.busy_timeout(std::time::Duration::from_secs(10))?;

  const CONFIG: &[&str] = &[
    "PRAGMA busy_timeout       = 10000",
    "PRAGMA journal_mode       = WAL",
    "PRAGMA journal_size_limit = 200000000",
    // Sync the file system less often.
    "PRAGMA synchronous        = NORMAL",
    "PRAGMA foreign_keys       = ON",
    "PRAGMA temp_store         = MEMORY",
    "PRAGMA cache_size         = -16000",
    // TODO: Maybe worth exploring once we have a benchmark, based on
    // https://phiresky.github.io/blog/2020/sqlite-performance-tuning/.
    // "PRAGMA mmap_size          = 30000000000",
    // "PRAGMA page_size          = 32768",

    // Safety feature around application-defined functions recommended by
    // https://sqlite.org/appfunc.html
    "PRAGMA trusted_schema     = OFF",
  ];

  // NOTE: we're querying here since some pragmas return data.
  for pragma in CONFIG {
    let mut stmt = conn.prepare(pragma)?;
    let mut rows = stmt.query([])?;
    rows.next()?;
  }

  if let Some(extensions) = extensions {
    for path in extensions {
      unsafe { conn.load_extension(path, None)? }
    }
  }

  // Initial optimize.
  conn.execute("PRAGMA optimize = 0x10002", ())?;

  return Ok(conn);
}

#[cfg(test)]
mod test {
  use super::*;
  use uuid::Uuid;

  #[test]
  fn test_connect_and_extensions() {
    let conn = connect_sqlite(None, None).unwrap();

    let row = conn
      .query_row(
        "SELECT (uuid_v7())",
        (),
        |row| -> rusqlite::Result<[u8; 16]> { row.get(0) },
      )
      .unwrap();

    let uuid = Uuid::from_bytes(row);
    assert_eq!(uuid.get_version_num(), 7);
    assert!(trailbase_extension::jsonschema::get_schema("std.FileUpload").is_some());

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

  #[tokio::test]
  async fn test_uuids() {
    let conn = crate::Connection::from_conn(connect_sqlite(None, None).unwrap()).unwrap();

    conn
      .execute(
        r#"CREATE TABLE test (
        id    BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
        text  TEXT
      )"#,
        (),
      )
      .await
      .unwrap();

    // V4 fails
    assert!(conn
      .execute(
        "INSERT INTO test (id) VALUES (?1) ",
        crate::params!(uuid::Uuid::new_v4().into_bytes())
      )
      .await
      .is_err());

    // V7 succeeds
    let id = uuid::Uuid::now_v7();
    assert!(conn
      .execute(
        "INSERT INTO test (id) VALUES (?1) ",
        crate::params!(id.into_bytes())
      )
      .await
      .is_ok());

    let read_id: uuid::Uuid = conn
      .query_value("SELECT id FROM test LIMIT 1", ())
      .await
      .unwrap()
      .unwrap();

    assert_eq!(id, read_id);

    let blob: Vec<u8> = conn
      .query_value("SELECT id FROM test LIMIT 1", ())
      .await
      .unwrap()
      .unwrap();

    assert_eq!(id, Uuid::from_slice(&blob).unwrap());

    let arr = conn
      .query_value::<[u8; 16]>("SELECT id FROM test LIMIT 1", ())
      .await;

    // FIXME: serde_rusqlite doesn't seem to be able to serialize blobs into [u8; N].
    assert!(arr.is_err());
  }
}
