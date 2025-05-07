use arc_swap::ArcSwap;
use maxminddb::{MaxMindDbError, Reader, geoip2::Country};
use rusqlite::Error;
use rusqlite::functions::Context;
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

pub(crate) fn geoip_country(context: &Context) -> Result<Option<String>, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let Some(text) = context.get_raw(0).as_str_or_null()? else {
    return Ok(None);
  };

  if !text.is_empty() {
    let client_ip: IpAddr = text.parse().map_err(|err| {
      Error::UserFunctionError(format!("Parsing ip '{text:?}' failed: {err}").into())
    })?;

    if let Some(ref reader) = **READER.load() {
      if let Ok(country) = reader.lookup::<Country>(client_ip) {
        return Ok(country.and_then(|c| Some(c.country?.iso_code?.to_string())));
      }
    }
  }

  Ok(None)
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
