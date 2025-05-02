use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::TableIndex;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::transaction::TransactionRecorder;

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct CreateIndexRequest {
  pub schema: TableIndex,
  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct CreateIndexResponse {
  pub sql: String,
}

pub async fn create_index_handler(
  State(state): State<AppState>,
  Json(request): Json<CreateIndexRequest>,
) -> Result<Json<CreateIndexResponse>, Error> {
  let dry_run = request.dry_run.unwrap_or(false);
  let index_name = request.schema.name.clone();
  let filename = format!("create_index_{index_name}");

  let create_index_query = request.schema.create_index_statement();

  if !dry_run {
    let create_index_query = create_index_query.clone();
    let conn = state.conn();
    let log = conn
      .call(move |conn| {
        let mut tx = TransactionRecorder::new(conn)?;

        tx.execute(&create_index_query, ())?;

        return tx
          .rollback()
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?;

    // Write to migration file.
    if let Some(log) = log {
      let migration_path = state.data_dir().migrations_path();
      log
        .apply_as_migration(conn, migration_path, &filename)
        .await?;
    }
  }

  return Ok(Json(CreateIndexResponse {
    sql: sqlformat::format(
      &format!("{create_index_query};"),
      &sqlformat::QueryParams::None,
      &sqlformat::FormatOptions {
        ignore_case_convert: None,
        indent: sqlformat::Indent::Spaces(2),
        uppercase: Some(true),
        lines_between_queries: 1,
      },
    ),
  }));
}
