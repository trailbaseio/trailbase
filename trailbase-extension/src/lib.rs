#![allow(clippy::needless_return)]

use rusqlite::functions::FunctionFlags;

use uuid::*;

pub mod jsonschema;
pub mod maxminddb;
pub mod password;

mod uuid;
mod validators;

pub fn sqlite3_extension_init(db: rusqlite::Connection) -> rusqlite::Result<rusqlite::Connection> {
  // WARN: Be careful with declaring INNOCUOUS. This allows these "app-defined functions" to run
  // even when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  db.create_scalar_function(
    "is_uuid",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    is_uuid,
  )?;
  db.create_scalar_function(
    "is_uuid_v7",
    1,
    FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS,
    is_uuid_v7,
  )?;
  db.create_scalar_function(
    "uuid_v7_text",
    0,
    FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
    uuid_v7_text,
  )?;

  db.create_scalar_function("uuid_v7", 0, FunctionFlags::SQLITE_INNOCUOUS, uuid_v7)?;
  db.create_scalar_function(
    "parse_uuid",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    parse_uuid,
  )?;

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
    validators::regexp,
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
    FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_INNOCUOUS,
    maxminddb::geoip_country,
  )?;

  return Ok(db);
}

#[cfg(test)]
pub(crate) fn connect() -> Result<rusqlite::Connection, rusqlite::Error> {
  let db = rusqlite::Connection::open_in_memory()?;
  return Ok(sqlite3_extension_init(db)?);
}
