use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::schema::TableIndex;
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

  let create_index_query = request.schema.create_index_statement();

  if !dry_run {
    let create_index_query = create_index_query.clone();
    let migration_path = state.data_dir().migrations_path();
    let conn = state.conn();
    let writer = conn
      .call(move |conn| {
        let mut tx =
          TransactionRecorder::new(conn, migration_path, format!("create_index_{index_name}"))?;

        tx.execute(&create_index_query)?;

        return tx
          .rollback_and_create_migration()
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await?;

    // Write to migration file.
    if let Some(writer) = writer {
      writer.write(conn).await?;
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
