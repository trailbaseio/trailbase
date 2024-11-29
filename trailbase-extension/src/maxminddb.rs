use maxminddb::{MaxMindDBError, Reader};
use parking_lot::Mutex;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Error as SqliteError};
use std::net::IpAddr;
use std::path::Path;
use std::sync::LazyLock;

static READER: LazyLock<Mutex<Option<Reader<Vec<u8>>>>> = LazyLock::new(|| Mutex::new(None));

pub fn load_geoip_db(path: impl AsRef<Path>) -> Result<(), MaxMindDBError> {
  let reader = Reader::open_readfile(path)?;
  *READER.lock() = Some(reader);
  return Ok(());
}

pub fn has_geoip_db() -> bool {
  return READER.lock().is_some();
}

pub(crate) fn geoip_country(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), SqliteError> {
  let client_ip_value = values
    .first()
    .ok_or_else(|| SqliteError::new_message("Missing argument"))?;
  if api::value_is_null(client_ip_value) {
    api::result_null(context);
    return Ok(());
  }

  let text = api::value_text(client_ip_value)?;
  if text.is_empty() {
    api::result_null(context);
    return Ok(());
  }

  let client_ip: IpAddr = text.parse().map_err(|err| {
    SqliteError::new_message(format!("Parsing ip '{client_ip_value:?}' failed: {err}"))
  })?;

  let cc: Option<String> = READER.lock().as_ref().and_then(|reader| {
    let country: maxminddb::geoip2::Country = reader.lookup(client_ip).ok()?;
    return Some(country.country?.iso_code?.to_owned());
  });

  match cc {
    Some(cc) => {
      api::result_text(context, cc)?;
    }
    None => {
      api::result_null(context);
    }
  };

  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_explicit_jsonschema() {
    let ip = "89.160.20.112";
    let conn = crate::connect().unwrap();

    let cc: Option<String> = conn
      .query_row(&format!("SELECT geoip_country('{ip}')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(cc, None);

    load_geoip_db("testdata/GeoIP2-Country-Test.mmdb").unwrap();

    let cc: String = conn
      .query_row(&format!("SELECT geoip_country('{ip}')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(cc, "SE");

    let cc: Option<String> = conn
      .query_row(&format!("SELECT geoip_country('127.0.0.1')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(cc, None);
  }
}
