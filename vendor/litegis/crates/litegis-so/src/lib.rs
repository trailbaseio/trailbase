#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]

use rusqlite::{Connection, ffi};

/// Entry point for SQLite to load the extension.
/// See <https://sqlite.org/c3ref/load_extension.html> on this function's name and usage.
/// # Safety
/// This function is called by SQLite and must be safe to call.
#[unsafe(no_mangle)]
unsafe extern "C" fn sqlite3_extension_init(
  db: *mut ffi::sqlite3,
  pz_err_msg: *mut *mut std::os::raw::c_char,
  p_api: *mut ffi::sqlite3_api_routines,
) -> std::os::raw::c_int {
  return unsafe {
    Connection::extension_init2(db, pz_err_msg, p_api, |conn| {
      litegis::register(&conn)?;

      // Returning false is the default leading to the extension being unloaded when the connection
      // is closed. Otherwise, SQLITE_OK_LOAD_PERMANENTLY will be emitted
      // (https://sqlite.org/rescode.html#ok_load_permanently).
      return Ok(false);
    })
  };
}
