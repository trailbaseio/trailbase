use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use sqlite3_parser::ast::Stmt;
use trailbase_schema::parse::{Bump, parse_into_statements};
use trailbase_schema::sqlite::Column;
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::AppState;
use crate::admin::AdminError as Error;
use crate::admin::util::{rows_to_columns, rows_to_sql_value_rows};
use crate::connection::{BuildOptions, ConnectionEntry};

#[derive(Debug, Deserialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct QueryRequest {
  /// The Query to execute. May be multiple statements separated by ";".
  query: String,
  /// Databases to attach.
  attached_databases: Option<Vec<String>>,
  /// Whether queries altering the schema should be rejected.
  #[serde(default)]
  allow_schema_alteration: bool,
}

#[derive(Debug, Default, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct QueryResponse {
  columns: Option<Vec<Column>>,

  rows: Vec<Vec<SqlValue>>,
}

#[utoipa::path(
  post,
  path = "/query",
  tag = "admin",
  request_body = QueryRequest,
  responses(
    (status = 200, description = "Success", body = QueryResponse),
    (status = 403, description = "Forbidden operations"),
    (status = 412, description = "Failed precondition: migration query not explicitly allowed"),
  )
)]
pub async fn query_handler(
  State(state): State<AppState>,
  Json(request): Json<QueryRequest>,
) -> Result<Json<QueryResponse>, Error> {
  let QueryRequest {
    query,
    attached_databases,
    allow_schema_alteration,
  } = request;

  if state
    .connection_manager()
    .main_entry()
    .connection
    .connection_type()
    != trailbase_sqlite::ConnectionType::Sqlite
  {
    return Err(Error::Internal(
      "query editor currently only supported for Sqlite".into(),
    ));
  }

  // Validate and check statements before executing anything.
  let must_invalidate_schema_cache = {
    let allocator = Bump::new();
    let statements =
      parse_into_statements(&allocator, &query).map_err(|err| Error::BadRequest(err.into()))?;

    let mut schema_alteration = false;
    let mut must_invalidate_schema_cache = false;

    for stmt in &statements {
      match stmt {
        Stmt::DropIndex { .. }
        | Stmt::DropTrigger { .. }
        | Stmt::CreateTrigger { .. }
        | Stmt::CreateIndex { .. } => {
          schema_alteration = true;
        }
        Stmt::DropView { .. }
        | Stmt::DropTable { .. }
        | Stmt::AlterTable { .. }
        | Stmt::CreateTable { .. }
        | Stmt::CreateVirtualTable { .. }
        | Stmt::CreateView { .. } => {
          schema_alteration = true;
          must_invalidate_schema_cache = true;
        }
        Stmt::Attach { .. } => {
          // Could allow access to local file-system, e.g. attach random SQLite databases or
          // files unrelated to TB via the admin UI.
          return Err(Error::Forbidden("Attach not allowed".into()));
        }
        Stmt::Detach { .. } => {
          return Err(Error::Forbidden("Detach not allowed".into()));
        }
        _ => {}
      }
    }

    if !allow_schema_alteration && schema_alteration {
      return Err(Error::Precondition(
        "Schema alterations need to be explicitly allowed".into(),
      ));
    }

    let readonly = statements
      .iter()
      .all(|stmt| matches!(stmt, Stmt::Select { .. }));
    if state.demo_mode() && !readonly {
      return Err(Error::Forbidden("Demo disallows mutation queries".into()));
    }

    must_invalidate_schema_cache
  };

  // Initialize a new connection, to avoid any sort of tomfoolery like dropping attached databases.
  // NOTE: This is relatively expensive, thus limit the number of spawned threads to 1.
  let ConnectionEntry {
    connection: conn, ..
  } = state
    .connection_manager()
    .build(BuildOptions {
      is_main: true,
      attached_databases: attached_databases.map(|v| v.into_iter().collect()),
      num_threads: Some(1),
    })
    .await?;

  let batched_rows_result = trailbase_sqlite::execute_batch(&conn, query).await;

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
