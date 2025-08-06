#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[no_mangle]
unsafe extern "C" fn init_sqlean_extension(
  db: *mut libsqlite3_sys::sqlite3,
  _pzErrMrg: *mut *mut ::std::os::raw::c_char,
  _pThunk: *const libsqlite3_sys::sqlite3_api_routines,
) -> ::std::os::raw::c_int {
  define_init(db as *mut sqlite3)
}

#[cfg(test)]
mod tests {
  use rusqlite::Connection;

  #[test]
  fn load_test() {
    unsafe {
      libsqlite3_sys::sqlite3_auto_extension(Some(super::init_sqlean_extension));
    };

    let conn = Connection::open_in_memory().unwrap();

    conn
      .query_row("SELECT define('sumn', ':n * (:n + 1) / 2')", (), |_row| {
        Ok(())
      })
      .unwrap();
    let sum: i64 = conn
      .query_row("SELECT sumn(5)", (), |row| row.get(0))
      .unwrap();
    assert_eq!(15, sum);
  }
}
