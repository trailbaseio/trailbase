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
  if state.demo_mode() {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let dry_run = request.dry_run.unwrap_or(false);
  let table_name = QualifiedName::parse(&request.name)?;
  let (unqualified_table_name, database_schema) = {
    (
      QualifiedName {
        name: table_name.name.clone(),
        database_schema: None,
      },
      table_name.database_schema.clone(),
    )
  };

  let (conn, migration_path) = super::get_conn_and_migration_path(&state, database_schema)?;
  let connection_metadata = super::build_connection_metadata(&state, &conn).await?;

  // QUESTION: Should we have a separate drop_view?
  let entity_type: &str = if connection_metadata
    .get_table(&unqualified_table_name)
    .is_some()
  {
    "TABLE"
  } else if connection_metadata
    .get_view(&unqualified_table_name)
    .is_some()
  {
    "VIEW"
  } else {
    return Err(Error::Precondition(format!(
      "Table or view '{table_name:?}' not found"
    )));
  };

  let tx_log = {
    let unqualified_table_name = unqualified_table_name.clone();
    conn
      .call(move |conn| {
        let mut tx = TransactionRecorder::new(conn)?;

        let query = format!(
          "DROP {entity_type} IF EXISTS {}",
          unqualified_table_name.escaped_string()
        );
        debug!("dropping table: {query}");
        tx.execute(&query, ())?;

        return tx
          .rollback()
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?
  };

  // Write migration file and apply it right away.
  if !dry_run && let Some(ref log) = tx_log {
    let filename =
      unqualified_table_name.migration_filename(&format!("drop_{}", entity_type.to_lowercase()));

    let _report = log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;

    // Fix configuration: remove all APIs reference the no longer existing table.
    {
      let mut config = state.get_config();
      let old_config_hash = hash_config(&config);

      config.record_apis.retain(|c| {
        if let Some(ref name) = c.table_name
          && let Ok(name) = QualifiedName::parse(name)
        {
          return name != table_name;
        }
        return true;
      });
      state
        .validate_and_update_config(config, Some(old_config_hash))
        .await?;
    }

    state.rebuild_connection_metadata().await?;
  }

  return Ok(Json(DropTableResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}
