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
  let (conn, _new_db) = crate::server::init_connection(
    state.data_dir(),
    state
      .get_config()
      .databases
      .iter()
      .flat_map(|d| d.name.clone())
      .collect(),
    state.runtime_root_fs(),
    state.json_schema_registry().clone(),
    state.dev_mode(),
  )
  .map_err(|err| Error::Precondition(err.to_string()))?;

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
