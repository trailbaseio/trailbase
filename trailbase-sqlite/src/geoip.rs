use std::path::PathBuf;

pub fn load_geoip_db(path: PathBuf) -> Result<(), String> {
  return trailbase_extension::maxminddb::load_geoip_db(path).map_err(|err| err.to_string());
}

pub fn has_geoip_db() -> bool {
  return trailbase_extension::maxminddb::has_geoip_db();
}
