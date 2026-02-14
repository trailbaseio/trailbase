use arc_swap::ArcSwap;
use maxminddb::{MaxMindDbError, Reader, geoip2};
use rusqlite::Error;
use rusqlite::functions::{Context, FunctionFlags};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::Path;
use std::sync::LazyLock;

type MaxMindReader = Reader<Vec<u8>>;

static READER: LazyLock<ArcSwap<Option<MaxMindReader>>> =
  LazyLock::new(|| ArcSwap::from_pointee(None));

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct City {
  pub country_code: Option<String>,
  pub name: Option<String>,
  pub subdivisions: Option<Vec<String>>,
}

impl City {
  fn from(city: &geoip2::City) -> Self {
    return Self {
      name: extract_city_name(city),
      country_code: city.country.iso_code.map(|c| c.to_string()),
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

#[derive(Clone, Debug, PartialEq)]
pub enum DatabaseType {
  Unknown,
  GeoLite2Country,
  GeoLite2City,
  GeoLite2ASN,
}

pub fn database_type() -> Option<DatabaseType> {
  if let Some(ref reader) = **READER.load() {
    return Some(match reader.metadata.database_type.as_str() {
      "GeoLite2-Country" => DatabaseType::GeoLite2Country,
      "GeoLite2-City" => DatabaseType::GeoLite2City,
      // Autonomous system number.
      "GeoLite2-ASN" => DatabaseType::GeoLite2ASN,
      _ => DatabaseType::Unknown,
    });
  }
  return None;
}

pub(crate) fn geoip_country(context: &Context) -> Result<Option<String>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(Some(country)) = reader
      .lookup(client_ip)
      .and_then(|result| result.decode::<geoip2::Country>())
    {
      return country.country.iso_code.map(|c| c.to_string());
    }

    return None;
  });
}

fn geoip_city(context: &Context) -> Result<Option<City>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(result) = reader.lookup(client_ip) {
      return result
        .decode::<geoip2::City>()
        .ok()
        .flatten()
        .map(|city| City::from(&city));
    }

    return None;
  });
}

pub(crate) fn geoip_city_json(context: &Context) -> Result<Option<String>, Error> {
  return Ok(geoip_city(context)?.and_then(|city| serde_json::to_string(&city).ok()));
}

pub(crate) fn geoip_city_name(context: &Context) -> Result<Option<String>, Error> {
  return geoip_extract(context, |reader, client_ip| {
    if let Ok(result) = reader.lookup(client_ip) {
      return result
        .decode::<geoip2::City>()
        .ok()
        .flatten()
        .and_then(|city| extract_city_name(&city));
    }

    return None;
  });
}

#[inline]
fn geoip_extract<T>(
  context: &Context,
  f: impl Fn(&MaxMindReader, IpAddr) -> Option<T>,
) -> Result<Option<T>, Error> {
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
  if let Some(city_name) = city.city.names.english {
    return Some(city_name.to_string());
  }
  return None;
}

fn extract_subdivision_names(city: &geoip2::City) -> Option<Vec<String>> {
  return Some(
    city
      .subdivisions
      .iter()
      .filter_map(|division| {
        return division.names.english.map(|en| en.to_string());
      })
      .collect(),
  );
}

pub(crate) fn register_extension_functions(db: &rusqlite::Connection) -> Result<(), Error> {
  // WARN: Be careful with declaring INNOCUOUS. It allows "user-defined functions" to run
  // when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  db.create_scalar_function(
    "geoip_country",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip_country,
  )?;
  db.create_scalar_function(
    "geoip_city_name",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip_city_name,
  )?;
  db.create_scalar_function(
    "geoip_city_json",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    geoip_city_json,
  )?;

  return Ok(());
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
