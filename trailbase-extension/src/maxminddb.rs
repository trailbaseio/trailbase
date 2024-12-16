use maxminddb::{MaxMindDBError, Reader};
use parking_lot::Mutex;
use rusqlite::functions::Context;
use rusqlite::types::ValueRef;
use rusqlite::Error;
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

pub(crate) fn geoip_country(context: &Context) -> rusqlite::Result<Option<String>> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  return match context.get_raw(0) {
    ValueRef::Null => Ok(None),
    ValueRef::Text(ascii) => {
      if ascii.is_empty() {
        return Ok(None);
      }

      let text = String::from_utf8_lossy(ascii);
      let client_ip: IpAddr = text.parse().map_err(|err| {
        Error::UserFunctionError(format!("Parsing ip '{text:?}' failed: {err}").into())
      })?;

      let cc: Option<String> = READER.lock().as_ref().and_then(|reader| {
        let country: maxminddb::geoip2::Country = reader.lookup(client_ip).ok()?;
        return Some(country.country?.iso_code?.to_owned());
      });

      Ok(cc)
    }
    arg => Err(Error::UserFunctionError(
      format!("Expected text, got {}", arg.data_type()).into(),
    )),
  };
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
