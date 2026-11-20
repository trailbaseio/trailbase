use axum::{Json, extract::State};
use itertools::Itertools;
use log::*;
use serde::Serialize;
use std::collections::BTreeSet;
use trailbase_schema::db::parse_sql::{Bump, parse_into_statement};
use trailbase_schema::db::sqlite::{QualifiedName, Table, TableIndex, View};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::connection::ConnectionEntry;
use crate::constants::SQLITE_SCHEMA_TABLE;

// TODO: Rudimentary unparsed trigger representation, since sqlparser didn't currently support
// parsing sqlite triggers. Now we're using sqlite3_parser and should return structured data
#[derive(Clone, Default, Debug, Serialize, TS, utoipa::ToSchema)]
pub struct TableTrigger {
  pub name: QualifiedName,
  pub table_name: String,
}

#[derive(Clone, Default, Debug, Serialize, TS, utoipa::ToSchema)]
pub struct Tenant {
  template: String,
  tenant: Option<String>,
}

#[derive(Clone, Default, Debug, Serialize, TS, utoipa::ToSchema)]
pub struct Schema<T: TS> {
  schema: T,
  sql: String,
  tenant: Option<Tenant>,
}

#[derive(Clone, Default, Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ListSchemasResponse {
  /// List of tuples of `Table` schema + `CREATE TABLE` query.
  pub tables: Vec<Schema<Table>>,
  /// List of tuples of `TableIndex` schema + `CREATE INDEX` query.
  pub indexes: Vec<Schema<TableIndex>>,
  /// List of tuples of `TableTrigger` schema + `CREATE Trigger` query.
  pub triggers: Vec<Schema<TableTrigger>>,
  /// List of tuples of `View` schema + `CREATE View` query.
  pub views: Vec<Schema<View>>,
}

#[utoipa::path(
  get,
  path = "/tables",
  tag = "admin",
  responses(
    (status = 200, description = "Success", body = ListSchemasResponse),
  )
)]
pub async fn list_tables_handler(
  State(state): State<AppState>,
) -> Result<Json<ListSchemasResponse>, Error> {
  let conn = &state.connection_manager().main_entry().connection;

  return match conn.connection_type() {
    #[cfg(feature = "pg")]
    trailbase_sqlite::ConnectionType::Pg => list_tables_handler_pg_impl(state).await,
    _ => list_tables_handler_sqlite_impl(state).await,
  };
}

#[cfg(feature = "pg")]
async fn list_tables_handler_pg_impl(state: AppState) -> Result<Json<ListSchemasResponse>, Error> {
  let ConnectionEntry {
    connection: conn, ..
  } = state.connection_manager().main_entry();

  let trailbase_schema::db::metadata::ConnectionMetadata { tables, views } =
    crate::schema_metadata::build_metadata_and_maybe_file_deletions(
      &conn,
      state.json_schema_registry().load_full(),
      /* setup_file_deletions = */ false,
    )
    .await?;

  let (indexes, triggers) = conn
    .call_writer(|mut conn| -> Result<_, trailbase_sqlite::Error> {
      let indexes = trailbase_pg_schema::build_all_index_schemas(&mut conn)
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      let triggers = trailbase_pg_schema::build_all_trigger_schemas(&mut conn)
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

      return Ok((indexes, triggers));
    })
    .await?;

  return Ok(Json(ListSchemasResponse {
    tables: tables
      .into_values()
      .map(|t| Schema {
        schema: t.schema,
        sql: "CREATE TABLE QUERY NOT AVAILABLE IN PG".to_string(),
        tenant: None,
      })
      .collect(),
    indexes: indexes
      .into_iter()
      .map(|(schema, sql)| Schema {
        schema,
        sql,
        tenant: None,
      })
      .collect(),
    triggers: triggers
      .into_iter()
      .map(|t| {
        return Schema {
          schema: TableTrigger {
            name: QualifiedName {
              name: t.trigger_name,
              database_schema: Some(t.trigger_schema),
            },
            table_name: t.event_object_table,
          },
          // NOTE: The action statement is not the full `CREATE TRIGGER`
          // statement like in SQLite.
          sql: t.action_statement,
          tenant: None,
        };
      })
      .collect(),
    views: views
      .into_values()
      .map(|v| Schema {
        schema: v.schema,
        sql: "CREATE VIEW QUERY NOT AVAILABLE IN PG".to_string(),
        tenant: None,
      })
      .collect(),
  }));
}

async fn list_tables_handler_sqlite_impl(
  state: AppState,
) -> Result<Json<ListSchemasResponse>, Error> {
  let mut response = ListSchemasResponse::default();

  let (databases, tenant_templates) =
    state.access_config(|c| (c.databases.clone(), c.tenant_templates.clone()));

  // Even if no additional DBs are attached in the config, we want to read "main".
  let mut database_names: Vec<_> = vec!["main".to_string()];
  database_names.extend(databases.into_iter().flat_map(|db| db.name));

  // Batch DBs, since a single SQLite connection can only support up to 125 DBs.
  for attached_dbs in database_names.into_iter().unique().batching(|it| {
    let batch: BTreeSet<String> = it.take(124).collect();
    return if batch.is_empty() { None } else { Some(batch) };
  }) {
    let has_main = attached_dbs.contains("main");

    let ConnectionEntry {
      connection: conn, ..
    } = state
      .connection_manager()
      .get_entry(/* attach_main= */ true, Some(attached_dbs))
      .await?;

    let databases = conn.list_databases().await?;
    for schema in fetch_schemas(&conn, &databases, /* skip_main= */ !has_main).await? {
      add_schema_to_response(schema, &mut response, None)?;
    }
  }

  // Also include the tenant DBs. We can go two routes here: I'd have this driven by the `_tenant`
  // table or just read the FS. We went with the latter since it seemed more robust. There may also
  // be value in displaying stragglers, e.g. tenants that were removed but the table still exists.
  //
  // TODO: It may be worth highlighting stragglers: *.db's w/o a corresponding _tenant entry.
  for tenant_template in tenant_templates
    .into_iter()
    .flat_map(|template| template.name)
    .unique()
  {
    let Ok(mut dir) = std::fs::read_dir(&state.data_dir().tenant_db_dir(&tenant_template)) else {
      warn!("failed to read data dir for tenant template: {tenant_template}");
      continue;
    };

    while let Some(Ok(entry)) = dir.next() {
      let Ok(metadata) = entry.metadata() else {
        continue;
      };

      if !metadata.is_file() && !metadata.is_symlink() {
        continue;
      }

      let path = entry.path();
      match (
        path.file_stem().and_then(|ext| ext.to_str()),
        path.extension().and_then(|ext| ext.to_str()),
      ) {
        (Some(stem), Some("db")) if stem != "golden" => {
          let Ok(tenant_id) = uuid::Uuid::parse_str(stem) else {
            warn!("Failed to parse tenant id");
            continue;
          };

          let ConnectionEntry {
            connection: conn, ..
          } = state
            .connection_manager()
            .build_for_tenant(&tenant_template, Some(tenant_id))
            .await?;

          let databases = conn.list_databases().await?;
          for schema in fetch_schemas(&conn, &databases, /* skip_main= */ true).await? {
            debug!("scanning {tenant_template}, {schema:?}");

            add_schema_to_response(
              schema,
              &mut response,
              Some(Tenant {
                template: tenant_template.clone(),
                tenant: Some(tenant_id.to_string()),
              }),
            )?;
          }
        }
        _ => {}
      }
    }
  }

  return Ok(Json(response));
}

#[derive(Debug)]
struct SqliteSchema {
  pub r#type: String,
  pub name: String,
  pub tbl_name: String,
  /// Create TABLE/VIEW/... query.
  pub sql: Option<String>,
  /// Connections schema name, e.g. "main", "other"
  pub db_schema: String,
}

async fn fetch_schemas(
  conn: &trailbase_sqlite::Connection,
  databases: &[trailbase_sqlite::Database],
  skip_main: bool,
) -> Result<Vec<SqliteSchema>, Error> {
  let mut schemas: Vec<SqliteSchema> = vec![];

  for db in databases {
    if skip_main && db.name == "main" {
      continue;
    }

    let table_and_view_list = conn
      .read_query_rows(
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
      .await?
      .into_iter()
      .map(|row| -> Result<SqliteSchema, trailbase_sqlite::Error> {
        return Ok(SqliteSchema {
          r#type: row.get(0)?,
          name: row.get(1)?,
          tbl_name: row.get(2)?,
          sql: row.get(3)?,
          db_schema: row.get(4)?,
        });
      })
      .collect::<Result<Vec<_>, _>>()?;

    schemas.extend(table_and_view_list);
  }

  return Ok(schemas);
}

fn add_schema_to_response(
  schema: SqliteSchema,
  response: &mut ListSchemasResponse,
  tenant: Option<Tenant>,
) -> Result<(), Error> {
  let SqliteSchema {
    r#type: type_name,
    name,
    tbl_name,
    sql,
    db_schema,
  } = schema;

  let build_db_schema = || match db_schema.as_str() {
    "" | "main" => None,
    db => Some(db.to_string()),
  };

  match type_name.as_str() {
    "table" => {
      let Some(ref sql) = sql else {
        warn!("Skipping missing sql for table: {name}");
        return Ok(());
      };

      let allocator = Bump::new();
      if let Some(create_table_statement) =
        parse_into_statement(&allocator, sql).map_err(|err| Error::Internal(err.into()))?
      {
        response.tables.push(Schema {
          schema: {
            let mut table: Table = (&create_table_statement).try_into()?;
            table.name.database_schema = build_db_schema();
            table
          },
          sql: sql.clone(),
          tenant,
        });
      }
    }
    "index" => {
      let Some(ref sql) = sql else {
        // Auto-indexes are expected to not have `.sql`.
        if !name.starts_with("sqlite_autoindex") {
          warn!("Skip missing sql for index: {name}");
        }
        return Ok(());
      };

      let allocator = Bump::new();
      if let Some(create_index_statement) =
        parse_into_statement(&allocator, sql).map_err(|err| Error::Internal(err.into()))?
      {
        response.indexes.push(Schema {
          schema: {
            let mut index: TableIndex = (&create_index_statement).try_into()?;
            index.name.database_schema = build_db_schema();
            index
          },
          sql: sql.clone(),
          tenant,
        });
      }
    }
    "view" => {
      let Some(ref sql) = sql else {
        warn!("Skip missing sql for view: {name}");
        return Ok(());
      };

      let allocator = Bump::new();
      if let Some(create_view_statement) =
        parse_into_statement(&allocator, sql).map_err(|err| Error::Internal(err.into()))?
      {
        let tables: Vec<_> = response
          .tables
          .iter()
          .filter_map(|Schema { schema: table, .. }| {
            if table.name.database_schema.as_deref().unwrap_or("main") == db_schema {
              return Some(table.clone());
            }
            return None;
          })
          .collect();

        response.views.push(Schema {
          schema: {
            let mut view = View::from(create_view_statement, &tables)?;
            view.name.database_schema = build_db_schema();
            view
          },
          sql: sql.clone(),
          tenant,
        });
      }
    }
    "trigger" => {
      let Some(sql) = sql else {
        warn!("Skipping empty trigger for: {name}");
        return Ok(());
      };

      // TODO: Turn this into structured data now that we use sqlite3_parser.
      response.triggers.push(Schema {
        schema: TableTrigger {
          name: QualifiedName {
            name,
            database_schema: build_db_schema(),
          },
          table_name: tbl_name,
        },
        sql,
        tenant,
      });
    }
    ty => warn!("Unknown schema type for '{}': {ty}", name),
  }

  return Ok(());
}
