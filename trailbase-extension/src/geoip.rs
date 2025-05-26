use arc_swap::ArcSwap;
use maxminddb::{MaxMindDbError, Reader, geoip2};
use rusqlite::Error;
use rusqlite::functions::Context;
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use std::sync::LazyLock;

type MaxMindReader = Reader<Vec<u8>>;

static READER: LazyLock<ArcSwap<Option<MaxMindReader>>> =
  LazyLock::new(|| ArcSwap::from_pointee(None));

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct City {
  country_code: Option<String>,
  name: Option<String>,
  subdivisions: Option<Vec<String>>,
}

impl City {
  fn from(city: &geoip2::City) -> Self {
    return Self {
      name: extract_city_name(city),
      country_code: city
        .country
        .as_ref()
        .and_then(|c| Some(c.iso_code?.to_string())),
      subdivisions: extract_subdivision_names(city),
    };
  }
}

pub fn load_geoip_db(path: impl AsRef<Path>) -> Result<(), MaxMindDbError> {
  let reader = Reader::open_readfile(path)?;
  READER.swap(Some(reader).into());
  return Ok(());
}

pub fn has_geoip_db() -> bool {
  return READER.load().is_some();
}

pub(crate) fn geoip_country(context: &Context) -> Result<Option<String>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(Some(country)) = reader.lookup::<geoip2::Country>(client_ip) {
      return Some(country.country?.iso_code?.to_string());
    }

    return None;
  });
}

pub(crate) fn geoip_city_json(context: &Context) -> Result<Option<String>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(Some(ref city)) = reader.lookup::<geoip2::City>(client_ip) {
      return serde_json::to_string(&City::from(city)).ok();
    }

    return None;
  });
}

pub(crate) fn geoip_city_name(context: &Context) -> Result<Option<String>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(Some(ref city)) = reader.lookup::<geoip2::City>(client_ip) {
      return extract_city_name(city);
    }

    return None;
  });
}

#[inline]
fn geoip_extract(
  context: &Context,
  f: impl Fn(&MaxMindReader, IpAddr) -> Option<String>,
) -> Result<Option<String>, Error> {
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
      return Ok(f(reader, client_ip));
    }
  }

  Ok(None)
}

fn extract_city_name(city: &geoip2::City) -> Option<String> {
  return city.city.as_ref().and_then(|c| {
    if let Some(ref names) = c.names {
      if let Some(city_name) = names.get("en") {
        return Some(city_name.to_string());
      }

      if let Some((_locale, city_name)) = names.first_key_value() {
        return Some(city_name.to_string());
      }
    }
    return None;
  });
}

fn extract_subdivision_names(city: &geoip2::City) -> Option<Vec<String>> {
  return city.subdivisions.as_ref().map(|divisions| {
    return divisions
      .iter()
      .filter_map(|s| {
        if let Some(ref names) = s.names {
          if let Some(city_name) = names.get("en") {
            return Some(city_name.to_string());
          }

          if let Some((_locale, city_name)) = names.first_key_value() {
            return Some(city_name.to_string());
          }
        }
        return None;
      })
      .collect();
  });
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

    load_geoip_db("testdata/GeoIP2-City-Test.mmdb").unwrap();

    let cc: String = conn
      .query_row(&format!("SELECT geoip_country('{ip}')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(cc, "SE");

    let city_name: String = conn
      .query_row(&format!("SELECT geoip_city_name('{ip}')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(city_name, "Linköping");

    let city: City = conn
      .query_row(&format!("SELECT geoip_city_json('{ip}')"), (), |row| {
        return Ok(serde_json::from_str(&row.get::<_, String>(0).unwrap()).unwrap());
      })
      .unwrap();

    assert_eq!(
      city,
      City {
        country_code: Some("SE".to_string()),
        name: Some("Linköping".to_string()),
        subdivisions: Some(vec!["Östergötland County".to_string()]),
      }
    );

    let cc: Option<String> = conn
      .query_row(&format!("SELECT geoip_country('127.0.0.1')"), (), |row| {
        row.get(0)
      })
      .unwrap();

    assert_eq!(cc, None);
  }
}
