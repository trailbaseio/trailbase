use axum::{Json, extract::State};
use log::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::sqlite::{Table, TableIndex, View, sqlite3_parse_into_statement};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::constants::SQLITE_SCHEMA_TABLE;

// TODO: Rudimentary unparsed trigger representation, since sqlparser didn't currently support
// parsing sqlite triggers. Now we're using sqlite3_parser and should return structured data
#[derive(Clone, Default, Debug, Serialize, TS)]
pub struct TableTrigger {
  pub name: String,
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

  let databases = state.conn().list_databases().await?;

  let mut schemas: Vec<SqliteSchema> = vec![];
  for db in databases {
    // NOTE: the "ORDER BY" is a bit sneaky, it ensures that we parse all "table"s before we parse
    // "view"s.
    let rows = state
      .conn()
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

    match schema.r#type.as_str() {
      "table" => {
        let table_name = &schema.name;
        let Some(sql) = schema.sql else {
          warn!("Missing sql for table: {table_name}");
          continue;
        };

        if let Some(create_table_statement) =
          sqlite3_parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
        {
          response.tables.push({
            let mut table: Table = create_table_statement.try_into()?;
            if schema.db_schema != "main" {
              table.name.database_schema = Some(schema.db_schema.clone());
            }
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
          sqlite3_parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
        {
          response.indexes.push(create_index_statement.try_into()?);
        }
      }
      "view" => {
        let view_name = &schema.name;
        let Some(sql) = schema.sql else {
          warn!("Missing sql for view: {view_name}");
          continue;
        };

        if let Some(create_view_statement) =
          sqlite3_parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
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
          response
            .views
            .push(View::from(create_view_statement, &tables)?);
        }
      }
      "trigger" => {
        let Some(sql) = schema.sql else {
          warn!("Empty trigger for: {schema:?}");
          continue;
        };

        // TODO: Turn this into structured data now that we use sqlite3_parser.
        response.triggers.push(TableTrigger {
          name: schema.name,
          table_name: schema.tbl_name,
          sql,
        });
      }
      x => warn!("Unknown schema type: {name} : {x}"),
    }
  }

  return Ok(Json(response));
}
