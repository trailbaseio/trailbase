use axum::{Json, extract::State};
use log::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::parse::parse_into_statement;
use trailbase_schema::sqlite::{QualifiedName, Table, TableIndex, View};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::constants::SQLITE_SCHEMA_TABLE;

// TODO: Rudimentary unparsed trigger representation, since sqlparser didn't currently support
// parsing sqlite triggers. Now we're using sqlite3_parser and should return structured data
#[derive(Clone, Default, Debug, Serialize, TS)]
pub struct TableTrigger {
  pub name: QualifiedName,
  pub table_name: String,
  pub sql: String,
}

#[derive(Clone, Default, Debug, Serialize, TS)]
#[ts(export)]
pub struct ListSchemasResponse {
  pub tables: Vec<Table>,
  pub indexes: Vec<TableIndex>,
  pub triggers: Vec<TableTrigger>,
  pub views: Vec<View>,
}

pub async fn list_tables_handler(
  State(state): State<AppState>,
) -> Result<Json<ListSchemasResponse>, Error> {
  #[derive(Debug, Deserialize)]
  pub struct SqliteSchema {
    pub r#type: String,
    pub name: String,
    pub tbl_name: String,
    pub sql: Option<String>,
    pub db_schema: String,
  }

  // FIXME: This is a hack. If there's a large number of databases, we'll miss schemas. At the same
  // time fixing this properly would probably also require overhauling our UI, e.g. display thing
  // hierarchically. What's the point of showing the same 100 schemas for thousands of tenants.
  let db_names: Vec<_> = state
    .get_config()
    .databases
    .iter()
    .flat_map(|d| d.name.clone())
    .collect();

  if db_names.len() > 124 {
    log::warn!("Not showing all DBs.")
  }

  let conn = state
    .connection_manager()
    .build(true, Some(&db_names.into_iter().take(124).collect()))?;
  let databases = conn.list_databases().await?;

  let mut schemas: Vec<SqliteSchema> = vec![];
  for db in databases {
    // NOTE: the "ORDER BY" is a bit sneaky, it ensures that we parse all "table"s before we parse
    // "view"s.
    let rows = conn
      .read_query_values::<SqliteSchema>(
        format!(
          "SELECT type, name, tbl_name, sql, ?1 AS db_schema FROM {}.{SQLITE_SCHEMA_TABLE} ORDER BY type",
          db.name
        ),
        trailbase_sqlite::params!(db.name),
      )
      .await?;

    schemas.extend(rows);
  }

  let mut response = ListSchemasResponse::default();

  for schema in schemas {
    let name = &schema.name;

    let db_schema = || -> Option<String> {
      match schema.db_schema.as_str() {
        "" | "main" => None,
        db => Some(db.to_string()),
      }
    };

    match schema.r#type.as_str() {
      "table" => {
        let table_name = &schema.name;
        let Some(sql) = schema.sql else {
          warn!("Missing sql for table: {table_name}");
          continue;
        };

        if let Some(create_table_statement) =
          parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
        {
          response.tables.push({
            let mut table: Table = create_table_statement.try_into()?;
            table.name.database_schema = db_schema();
            table
          });
        }
      }
      "index" => {
        let index_name = &schema.name;
        let Some(sql) = schema.sql else {
          // Auto-indexes are expected to not have `.sql`.
          if !name.starts_with("sqlite_autoindex") {
            warn!("Missing sql for index: {index_name}");
          }
          continue;
        };

        if let Some(create_index_statement) =
          parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
        {
          let mut index: TableIndex = create_index_statement.try_into()?;
          index.name.database_schema = db_schema();
          response.indexes.push(index);
        }
      }
      "view" => {
        let view_name = &schema.name;
        let Some(sql) = schema.sql else {
          warn!("Missing sql for view: {view_name}");
          continue;
        };

        if let Some(create_view_statement) =
          parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
        {
          let tables: Vec<_> = response
            .tables
            .iter()
            .filter_map(|table| {
              if table.name.database_schema.as_deref().unwrap_or("main") == schema.db_schema {
                return Some(table.clone());
              }
              return None;
            })
            .collect();

          let mut view = View::from(create_view_statement, &tables)?;
          view.name.database_schema = db_schema();
          response.views.push(view);
        }
      }
      "trigger" => {
        let Some(sql) = schema.sql else {
          warn!("Empty trigger for: {schema:?}");
          continue;
        };

        // TODO: Turn this into structured data now that we use sqlite3_parser.
        response.triggers.push(TableTrigger {
          name: QualifiedName {
            name: schema.name,
            database_schema: db_schema(),
          },
          table_name: schema.tbl_name,
          sql,
        });
      }
      x => warn!("Unknown schema type: {name} : {x}"),
    }
  }

  return Ok(Json(response));
}
