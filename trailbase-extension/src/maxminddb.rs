use arc_swap::ArcSwap;
use maxminddb::{MaxMindDbError, Reader};
use rusqlite::Error;
use rusqlite::functions::Context;
use rusqlite::types::ValueRef;
use std::net::IpAddr;
use std::path::Path;
use std::sync::LazyLock;

type MaxMindReader = Reader<Vec<u8>>;

static READER: LazyLock<ArcSwap<Option<MaxMindReader>>> =
  LazyLock::new(|| ArcSwap::from_pointee(None));

pub fn load_geoip_db(path: impl AsRef<Path>) -> Result<(), MaxMindDbError> {
  let reader = Reader::open_readfile(path)?;
  READER.swap(Some(reader).into());
  return Ok(());
}

pub fn has_geoip_db() -> bool {
  return READER.load().is_some();
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

      if let Some(ref reader) = **READER.load() {
        use maxminddb::geoip2::Country;
        if let Ok(country) = reader.lookup::<Country>(client_ip) {
          return Ok(country.and_then(|c| Some(c.country?.iso_code?.to_string())));
        }
      }

      Ok(None)
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
    let conn = crate::connect_sqlite(None, None).unwrap();

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
