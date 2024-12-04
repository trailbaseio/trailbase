#![allow(clippy::needless_return)]

pub mod schema;

pub use schema::set_user_schemas;

use std::path::PathBuf;

pub fn load_geoip_db(path: PathBuf) -> Result<(), String> {
  return trailbase_extension::maxminddb::load_geoip_db(path).map_err(|err| err.to_string());
}

pub fn has_geoip_db() -> bool {
  return trailbase_extension::maxminddb::has_geoip_db();
}

#[no_mangle]
unsafe extern "C" fn init_trailbase_extension(
  db: *mut rusqlite::ffi::sqlite3,
  pz_err_msg: *mut *mut ::std::os::raw::c_char,
  p_thunk: *const rusqlite::ffi::sqlite3_api_routines,
) -> ::std::os::raw::c_int {
  // Add sqlite-vec extension.
  sqlite_vec::sqlite3_vec_init();

  // Add trailbase-extensions.
  return trailbase_extension::sqlite3_extension_init(
    db,
    pz_err_msg as *mut *mut ::std::os::raw::c_char,
    p_thunk as *mut rusqlite::ffi::sqlite3_api_routines,
  ) as ::std::os::raw::c_int;
}

pub fn connect_sqlite(
  path: Option<PathBuf>,
  extensions: Option<Vec<PathBuf>>,
) -> Result<rusqlite::Connection, rusqlite::Error> {
  schema::try_init_schemas();

  unsafe { rusqlite::ffi::sqlite3_auto_extension(Some(init_trailbase_extension)) };

  let conn = if let Some(p) = path {
    use rusqlite::OpenFlags;
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
      | OpenFlags::SQLITE_OPEN_CREATE
      | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    rusqlite::Connection::open_with_flags(p, flags)?
  } else {
    rusqlite::Connection::open_in_memory()?
  };

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
  fn test_connect() {
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
  }
}
