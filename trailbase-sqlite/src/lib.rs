#![allow(clippy::needless_return)]

pub mod schema;

pub use schema::set_user_schemas;

use std::path::PathBuf;

#[no_mangle]
unsafe extern "C" fn init_extension(
  db: *mut libsql::ffi::sqlite3,
  pz_err_msg: *mut *const ::std::os::raw::c_char,
  p_thunk: *const libsql::ffi::sqlite3_api_routines,
) -> ::std::os::raw::c_int {
  return trailbase_extension::sqlite3_extension_init(
    db,
    pz_err_msg as *mut *mut ::std::os::raw::c_char,
    p_thunk as *mut libsql::ffi::sqlite3_api_routines,
  ) as ::std::os::raw::c_int;
}

// Lightweight optimization on db connect based on $2.1: https://sqlite.org/lang_analyze.html
async fn initial_optimize(conn: &libsql::Connection) -> Result<(), libsql::Error> {
  conn.execute("PRAGMA optimize = 0x10002", ()).await?;
  return Ok(());
}

pub async fn connect_sqlite(
  path: Option<PathBuf>,
  extensions: Option<Vec<PathBuf>>,
) -> Result<libsql::Connection, libsql::Error> {
  schema::try_init_schemas();

  // NOTE:  We need libsql to initialize some internal variables before auto_extension works
  // reliably. That's why we're creating a throw-away connection first. Haven't debugged this
  // further but see error message below.
  //
  // thread 'main' panicked at
  // /.../libsql-0.5.0-alpha.2/src/local/database.rs:209:17: assertion `left == right` failed:
  //
  // libsql was configured with an incorrect threading configuration and the api is not safe to
  // use. Please check that no multi-thread options have been set. If nothing was configured then
  // please open an issue at: https://github.com/libsql/libsql
  //   left: 21
  //   right: 0
  drop(
    libsql::Builder::new_local(":memory:")
      .build()
      .await
      .unwrap()
      .connect(),
  );

  let p: PathBuf = path.unwrap_or_else(|| PathBuf::from(":memory:"));
  let builder = libsql::Builder::new_local(p).build().await?;

  unsafe { libsql::ffi::sqlite3_auto_extension(Some(init_extension)) };

  let conn = builder.connect()?;

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

  // NOTE: we're querying here since some pragmas return data. However, libsql doesn't like
  // executed statements to return rows.
  for pragma in CONFIG {
    conn.query(pragma, ()).await?;
  }

  if let Some(extensions) = extensions {
    for path in extensions {
      conn.load_extension(path, None)?;
    }
  }

  initial_optimize(&conn).await?;

  return Ok(conn);
}

pub async fn query_one_row(
  conn: &libsql::Connection,
  sql: &str,
  params: impl libsql::params::IntoParams,
) -> Result<libsql::Row, libsql::Error> {
  let mut rows = conn.query(sql, params).await?;
  let row = rows.next().await?.ok_or(libsql::Error::QueryReturnedNoRows);
  return row;
}

pub async fn query_row(
  conn: &libsql::Connection,
  sql: &str,
  params: impl libsql::params::IntoParams,
) -> Result<Option<libsql::Row>, libsql::Error> {
  let mut rows = conn.query(sql, params).await?;
  return rows.next().await;
}

#[cfg(test)]
mod test {
  use super::*;
  use uuid::Uuid;

  #[tokio::test]
  async fn test_connect() {
    let conn = connect_sqlite(None, None).await.unwrap();

    let row = query_one_row(&conn, "SELECT (uuid_v7())", ())
      .await
      .unwrap();

    let uuid = Uuid::from_bytes(row.get::<[u8; 16]>(0).unwrap());

    assert_eq!(uuid.get_version_num(), 7);

    assert!(trailbase_extension::jsonschema::get_schema("std.FileUpload").is_some());
  }
}
