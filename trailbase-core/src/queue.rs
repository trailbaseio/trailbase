use serde::{Deserialize, Serialize};
use thiserror::Error;
use trailbase_apalis::sqlite::SqliteStorage;

use crate::data_dir::DataDir;

#[derive(Debug, Error)]
pub enum QueueError {
  #[error("SQLite ext error: {0}")]
  SqliteExtension(#[from] trailbase_extension::Error),
  #[error("SQLite error: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Job {
  SendEmail(),
}

pub type QueueStorage = SqliteStorage<Job>;

pub(crate) async fn init_queue_storage(
  data_dir: Option<&DataDir>,
) -> Result<QueueStorage, QueueError> {
  let queue_path = data_dir.map(|d| d.queue_db_path());
  let conn = trailbase_sqlite::Connection::new(
    || -> Result<_, trailbase_sqlite::Error> {
      return Ok(
        crate::connection::connect_rusqlite_without_default_extensions_and_schemas(
          queue_path.clone(),
        )?,
      );
    },
    None,
  )?;

  SqliteStorage::setup(&conn).await?;

  let config = trailbase_apalis::Config::new("apalis::test");
  return Ok(SqliteStorage::new_with_config(conn, config));
}
