use std::path::PathBuf;

pub fn connect_sqlite(
  path: Option<PathBuf>,
  extensions: Option<Vec<PathBuf>>,
) -> Result<rusqlite::Connection, trailbase_extension::Error> {
  trailbase_schema::registry::try_init_schemas();

  return trailbase_extension::connect_sqlite(path, extensions);
}
