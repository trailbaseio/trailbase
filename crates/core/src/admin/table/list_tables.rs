use axum::{Json, extract::State};
use itertools::Itertools;
use log::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use trailbase_schema::parse::parse_into_statement;
use trailbase_schema::sqlite::{QualifiedName, Table, TableIndex, View};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::connection::{BuildOptions, ConnectionEntry};
use crate::constants::SQLITE_SCHEMA_TABLE;

// TODO: Rudimentary unparsed trigger representation, since sqlparser didn't currently support
// parsing sqlite triggers. Now we're using sqlite3_parser and should return structured data
#[derive(Clone, Default, Debug, Serialize, TS)]
pub struct TableTrigger {
  pub name: QualifiedName,
  pub table_name: String,
}

#[derive(Clone, Default, Debug, Serialize, TS)]
#[ts(export)]
pub struct ListSchemasResponse {
  pub tables: Vec<(Table, String)>,
  pub indexes: Vec<(TableIndex, String)>,
  pub triggers: Vec<(TableTrigger, String)>,
  pub views: Vec<(View, String)>,
}

pub async fn list_tables_handler(
  State(state): State<AppState>,
) -> Result<Json<ListSchemasResponse>, Error> {
  #[derive(Debug, Deserialize)]
  struct SqliteSchema {
    pub r#type: String,
    pub name: String,
    pub tbl_name: String,
    /// Create TABLE/VIEW/... query.
    pub sql: Option<String>,
    /// Connections schema name, e.g. "main", "other"
    pub db_schema: String,
  }

  let db_names: BTreeSet<String> = {
    let mut db_names = BTreeSet::from(["main".to_string()]);
    db_names.extend(
      state
        .get_config()
        .databases
        .iter()
        .flat_map(|d| d.name.clone()),
    );

    db_names
  };

  // Batch DBs, since a single SQLite connection can only support up to 125 DBs.
  let mut response = ListSchemasResponse::default();
  for attached_dbs in db_names.into_iter().batching(|it| {
    let batch: BTreeSet<String> = it.take(124).collect();
    return if batch.is_empty() { None } else { Some(batch) };
  }) {
    let ConnectionEntry {
      connection: conn, ..
    } = state
      .connection_manager()
      .get_entry(BuildOptions {
        is_main: true,
        attached_databases: Some(attached_dbs),
        ..Default::default()
      })
      .await?;

    let databases = conn.list_databases().await?;

    let mut schemas: Vec<SqliteSchema> = vec![];
    for db in databases {
      let table_and_view_list = conn
        .read_query_values::<SqliteSchema>(
          // NOTE: the "ORDER BY" is a bit sneaky, it ensures that we parse all "table"s before we
          // parse "view"s.
          format!(
            r#"
               SELECT type, name, tbl_name, sql, "{db}" AS db_schema
                 FROM "{db}"."{SQLITE_SCHEMA_TABLE}"
                 ORDER BY type;
            "#,
            db = db.name
          ),
          (),
        )
        .await?;

      schemas.extend(table_and_view_list);
    }

    for schema in schemas {
      let db_schema: Option<String> = match schema.db_schema.as_str() {
        "" | "main" => None,
        db => Some(db.to_string()),
      };

      match schema.r#type.as_str() {
        "table" => {
          let table_name = &schema.name;
          // if table

          let Some(sql) = schema.sql else {
            warn!("Missing sql for table: {table_name}");
            continue;
          };

          if let Some(create_table_statement) =
            parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
          {
            response.tables.push((
              {
                let mut table: Table = create_table_statement.try_into()?;
                table.name.database_schema = db_schema;
                table
              },
              sql,
            ));
          }
        }
        "index" => {
          let index_name = &schema.name;
          let Some(sql) = schema.sql else {
            // Auto-indexes are expected to not have `.sql`.
            if !index_name.starts_with("sqlite_autoindex") {
              warn!("Missing sql for index: {index_name}");
            }
            continue;
          };

          if let Some(create_index_statement) =
            parse_into_statement(&sql).map_err(|err| Error::Internal(err.into()))?
          {
            response.indexes.push((
              {
                let mut index: TableIndex = create_index_statement.try_into()?;
                index.name.database_schema = db_schema;
                index
              },
              sql,
            ));
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
              .filter_map(|(table, _)| {
                if table.name.database_schema.as_deref().unwrap_or("main") == schema.db_schema {
                  return Some(table.clone());
                }
                return None;
              })
              .collect();

            response.views.push((
              {
                let mut view = View::from(create_view_statement, &tables)?;
                view.name.database_schema = db_schema;
                view
              },
              sql,
            ));
          }
        }
        "trigger" => {
          let Some(sql) = schema.sql else {
            warn!("Empty trigger for: {schema:?}");
            continue;
          };

          // TODO: Turn this into structured data now that we use sqlite3_parser.
          response.triggers.push((
            TableTrigger {
              name: QualifiedName {
                name: schema.name,
                database_schema: db_schema,
              },
              table_name: schema.tbl_name,
            },
            sql,
          ));
        }
        ty => warn!("Unknown schema type for '{}': {ty}", schema.name),
      }
    }
  }

  return Ok(Json(response));
}
