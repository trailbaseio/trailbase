#![allow(clippy::needless_return)]

use sqlite_loadable::prelude::*;
use sqlite_loadable::{define_scalar_function, define_scalar_void_function};
use uuid::*;

pub mod jsonschema;
pub mod maxminddb;
pub mod password;

mod uuid;
mod validators;

pub use sqlite_loadable::ext::sqlite3;
pub use sqlite_loadable::ext::sqlite3_api_routines;

#[sqlite_entrypoint]
pub fn sqlite3_extension_init(db: *mut sqlite3) -> Result<(), sqlite_loadable::Error> {
  // WARN: Be careful with declaring INNOCUOUS. This allows these "app-defined functions" to run
  // even when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  // UUID
  define_scalar_void_function(
    db,
    "is_uuid",
    1,
    is_uuid,
    FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_void_function(
    db,
    "is_uuid_v7",
    1,
    is_uuid_v7,
    FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_function(
    db,
    "uuid_url_safe_b64",
    1,
    uuid_url_safe_b64,
    FunctionFlags::DETERMINISTIC | FunctionFlags::UTF8 | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_function(
    db,
    "uuid_v7_text",
    0,
    uuid_v7_text,
    FunctionFlags::UTF8 | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_void_function(db, "uuid_v7", 0, uuid_v7, FunctionFlags::INNOCUOUS)?;
  define_scalar_function(
    db,
    "parse_uuid",
    1,
    parse_uuid,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;

  define_scalar_function(
    db,
    "hash_password",
    1,
    password::hash_password_sqlite,
    FunctionFlags::UTF8 | FunctionFlags::INNOCUOUS,
  )?;

  // Match column against given JSON schema, e.g. jsonschema_matches(col, '<schema>').
  define_scalar_function(
    db,
    "jsonschema_matches",
    2,
    jsonschema::jsonschema_matches,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  // Match column against registered JSON schema by name, e.g. jsonschema(col, 'schema-name').
  define_scalar_function(
    db,
    "jsonschema",
    2,
    jsonschema::jsonschema_by_name,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_function(
    db,
    "jsonschema",
    3,
    jsonschema::jsonschema_by_name_with_extra_args,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;

  // Validators for CHECK constraints.
  define_scalar_function(
    db,
    // NOTE: the name needs to be "regexp" to be picked up by sqlites REGEXP matcher:
    // https://www.sqlite.org/lang_expr.html
    "regexp",
    2,
    validators::regexp,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_function(
    db,
    "is_email",
    1,
    validators::is_email,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  // NOTE: there's also https://sqlite.org/json1.html#jvalid
  define_scalar_function(
    db,
    "is_json",
    1,
    validators::is_json,
    FunctionFlags::UTF8 | FunctionFlags::DETERMINISTIC | FunctionFlags::INNOCUOUS,
  )?;
  define_scalar_function(
    db,
    "geoip_country",
    1,
    maxminddb::geoip_country,
    FunctionFlags::UTF8 | FunctionFlags::INNOCUOUS,
  )?;

  Ok(())
}

#[cfg(test)]
unsafe extern "C" fn init_trailbase_extension_for_test(
  db: *mut rusqlite::ffi::sqlite3,
  pz_err_msg: *mut *mut ::std::os::raw::c_char,
  p_thunk: *const rusqlite::ffi::sqlite3_api_routines,
) -> ::std::os::raw::c_int {
  return sqlite3_extension_init(
    db,
    pz_err_msg,
    p_thunk as *mut rusqlite::ffi::sqlite3_api_routines,
  ) as ::std::os::raw::c_int;
}

#[cfg(test)]
pub(crate) fn connect() -> Result<rusqlite::Connection, rusqlite::Error> {
  unsafe {
    rusqlite::ffi::sqlite3_auto_extension(Some(init_trailbase_extension_for_test));
  }

  return Ok(rusqlite::Connection::open_in_memory()?);
}
