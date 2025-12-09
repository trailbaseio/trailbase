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
  attached_databases: Option<Vec<String>>,
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
      Stmt::Attach { .. } => {
        // Could allow access to local file-system, e.g. attach random SQLite databases or
        // files unrelated to TB via the admin UI.
        return Err(Error::Precondition("Attach not allowed".into()));
      }
      Stmt::Detach { .. } => {
        return Err(Error::Precondition("Detach not allowed".into()));
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
  let conn = state.connection_manager().build(
    true,
    request
      .attached_databases
      .map(|v| v.into_iter().collect())
      .as_ref(),
  )?;

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
