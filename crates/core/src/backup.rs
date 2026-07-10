use trailbase_sqlite::Error;

use crate::AppState;
use crate::connection::{BuildOptions, ConnectionError};

async fn backup_all(state: &AppState) -> Result<(), ConnectionError> {
  let config = state.get_config();
  let mgr = state.connection_manager();
  if !matches!(
    mgr.main_entry().connection.connection_type(),
    trailbase_sqlite::ConnectionType::Sqlite
  ) {
    return Err(ConnectionError::InvalidSetting(
      "Only sqlite supported for now",
    ));
  }

  let attached_dbs: Vec<String> = config
    .record_apis
    .iter()
    .map(|c| c.attached_databases.clone())
    .flatten()
    .collect();

  let dbs: Vec<String> = [
    vec![
      "main".to_string(),
      "logs".to_string(),
      "session".to_string(),
    ],
    attached_dbs,
  ]
  .into_iter()
  .flatten()
  .collect();

  let now = chrono::Utc::now();
  let target_path = state.data_dir().backup_path().join(now.to_rfc3339());

  for db in dbs {
    let conn = mgr
      .get_entry(BuildOptions {
        is_main: db == "main",
        attached_databases: None,
        num_threads: Some(1),
      })
      .await?;
  }

  return Ok(());
}
