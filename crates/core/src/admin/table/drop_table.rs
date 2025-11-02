use axum::extract::{Json, State};
use log::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::QualifiedName;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::hash_config;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct DropTableRequest {
  // TODO: Should be fully qualified.
  pub name: String,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct DropTableResponse {
  pub sql: String,
}

pub async fn drop_table_handler(
  State(state): State<AppState>,
  Json(request): Json<DropTableRequest>,
) -> Result<Json<DropTableResponse>, Error> {
  let unqualified_table_name = request.name.to_string();
  if state.demo_mode() && unqualified_table_name.starts_with("_") {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);
  let table_name = QualifiedName::parse(&request.name)?;

  let entity_type: &str;
  if state.connection_metadata().get_table(&table_name).is_some() {
    entity_type = "TABLE";
  } else if state.connection_metadata().get_view(&table_name).is_some() {
    entity_type = "VIEW";
  } else {
    return Err(Error::Precondition(format!(
      "Table or view '{table_name:?}' not found"
    )));
  }
  let filename = table_name.migration_filename(&format!("drop_{}", entity_type.to_lowercase()));

  let tx_log = state
    .conn()
    .call(move |conn| {
      let mut tx = TransactionRecorder::new(conn)?;

      let query = format!(
        "DROP {entity_type} IF EXISTS {table_name}",
        table_name = table_name.escaped_string()
      );
      debug!("dropping table: {query}");
      tx.execute(&query, ())?;

      return tx
        .rollback()
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
    })
    .await?;

  if !dry_run {
    // Write migration file and apply it right away.
    if let Some(ref log) = tx_log {
      let migration_path = state.data_dir().migrations_path();
      let _report = log
        .apply_as_migration(state.conn(), migration_path, &filename)
        .await?;
    }

    state.rebuild_connection_metadata().await?;

    // Fix configuration: remove all APIs reference the no longer existing table.
    let mut config = state.get_config();
    let old_config_hash = hash_config(&config);

    config.record_apis.retain(|c| {
      if let Some(ref name) = c.table_name {
        return *name != unqualified_table_name;
      }
      return true;
    });
    state
      .validate_and_update_config(config, Some(old_config_hash))
      .await?;
  }

  return Ok(Json(DropTableResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
