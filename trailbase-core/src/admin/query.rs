use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::schema::Column;
use crate::table_metadata::sqlite3_parse_into_statements;

#[derive(Debug, Default, Serialize, TS)]
#[ts(export)]
pub struct QueryResponse {
  columns: Option<Vec<Column>>,

  #[ts(type = "Object[][]")]
  rows: Vec<Vec<serde_json::Value>>,
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
  // NOTE: conn.query() only executes the first query and quietly drops the rest :/.
  //
  // In an ideal world we'd use sqlparser to validate the entire query before doing anything *and*
  // also to break up the statements and execute them one-by-one. However, sqlparser is far from
  // having 100% coverage, for example, it doesn't parse trigger statements (maybe
  // https://crates.io/crates/sqlite3-parser would have been the better choice).
  //
  // In the end we really want to allow executing all constructs as valid to sqlite. As such we
  // best effort parse the statements to see if need to invalidate the table cache and otherwise
  // fall back to execute batch which materializes all rows and invalidate anyway.

  // Check the statements are correct before executing anything, just to be sure.
  let statements = sqlite3_parse_into_statements(&request.query)?;
  let mut must_invalidate_table_cache = false;
  for stmt in statements {
    use sqlite3_parser::ast::Stmt;

    match stmt {
      Stmt::DropView { .. }
      | Stmt::DropTable { .. }
      | Stmt::AlterTable { .. }
      | Stmt::CreateTable { .. }
      | Stmt::CreateVirtualTable { .. }
      | Stmt::CreateView { .. } => {
        must_invalidate_table_cache = true;
      }
      _ => {
        // Do nothing.
      }
    }
  }

  let batched_rows_result = state.conn().execute_batch(&request.query).await;

  // In the fallback case we always need to invalidate the cache.
  if must_invalidate_table_cache {
    state.table_metadata().invalidate_all().await?;
  }

  if let Some(rows) = batched_rows_result? {
    let (rows, columns) = rows_to_json_arrays(rows, 1024)?;

    return Ok(Json(QueryResponse { columns, rows }));
  }
  return Ok(Json(QueryResponse::default()));
}
