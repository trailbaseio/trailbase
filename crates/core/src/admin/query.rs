use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use trailbase_schema::parse::parse_into_statements;
use trailbase_schema::sqlite::Column;
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::AppState;
use crate::admin::AdminError as Error;
use crate::admin::util::{rows_to_columns, rows_to_sql_value_rows};

#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct QueryResponse {
  columns: Option<Vec<Column>>,

  rows: Vec<Vec<SqlValue>>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct QueryRequest {
  query: String,
}

pub async fn query_handler(
  State(state): State<AppState>,
  Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, Error> {
  // Check the statements are correct before executing anything, just to be sure.
  let statements =
    parse_into_statements(&request.query).map_err(|err| Error::BadRequest(err.into()))?;

  let mut must_invalidate_schema_cache = false;
  let mut mutation = true;

  for stmt in statements {
    use sqlite3_parser::ast::Stmt;

    match stmt {
      Stmt::DropView { .. }
      | Stmt::DropTable { .. }
      | Stmt::AlterTable { .. }
      | Stmt::CreateTable { .. }
      | Stmt::CreateVirtualTable { .. }
      | Stmt::CreateView { .. } => {
        must_invalidate_schema_cache = true;
      }
      Stmt::Select { .. } => {
        mutation = false;
      }
      _ => {}
    }
  }

  if state.demo_mode() && mutation {
    return Err(Error::Precondition(
      "Demo disallows mutation queries".into(),
    ));
  }

  // Initialize a new connection, to avoid any sort of tomfoolery like dropping attached databases.
  let (conn, _new_db) = {
    let sync_wasm_runtimes = crate::wasm::build_sync_wasm_runtimes_for_components(
      state.data_dir().root().join("wasm"),
      state.runtime_root_fs(),
      state.dev_mode(),
    )
    .map_err(|err| Error::Precondition(err.to_string()))?;

    crate::connection::init_main_db(
      Some(state.data_dir()),
      Some(state.json_schema_registry().clone()),
      state
        .get_config()
        .databases
        .iter()
        .flat_map(|d| {
          d.name
            .as_ref()
            .map(|n| crate::connection::AttachedDatabase::from_data_dir(state.data_dir(), n))
        })
        .collect(),
      sync_wasm_runtimes,
    )
    .map_err(|err| Error::Precondition(err.to_string()))?
  };

  let batched_rows_result = conn.execute_batch(request.query).await;

  // In the fallback case we always need to invalidate the cache.
  if must_invalidate_schema_cache {
    state.rebuild_connection_metadata().await?;
  }

  let batched_rows = batched_rows_result.map_err(|err| Error::BadRequest(err.into()))?;
  if let Some(rows) = batched_rows {
    return Ok(Json(QueryResponse {
      columns: Some(rows_to_columns(&rows)),
      rows: rows_to_sql_value_rows(&rows)?,
    }));
  }
  return Ok(Json(QueryResponse::default()));
}
