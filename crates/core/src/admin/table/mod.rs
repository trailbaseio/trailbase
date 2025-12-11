// Indexes
mod alter_index;
mod create_index;
mod drop_index;

pub(super) use alter_index::alter_index_handler;
pub(super) use create_index::create_index_handler;
pub(super) use drop_index::drop_index_handler;

// Tables
mod alter_table;
mod create_table;
mod drop_table;

pub(crate) use alter_table::alter_table_handler;
#[allow(unused)]
pub(crate) use create_table::{CreateTableRequest, create_table_handler};
pub(crate) use drop_table::drop_table_handler;

// Lists both Tables and Indexes
mod list_tables;

pub(crate) use list_tables::list_tables_handler;

/// Builds dedicated connection for database with given name.
///
/// NOTE: We cannot use the ConnectionManager's facilities since migrations require DBs to be
/// attached as "main". Otherwise, the migrations themselves would need fully-qualified statements,
/// which would be problematic for renames and multi-tenancy.
fn get_conn_and_migration_path(
  state: &crate::AppState,
  db: Option<String>,
) -> Result<(trailbase_sqlite::Connection, std::path::PathBuf), crate::admin::AdminError> {
  return match db {
    Some(db) if db != "main" => {
      let db_path = state.data_dir().data_path().join(format!("{db}.db"));
      let migration_path = state.data_dir().migrations_path().join(&db);
      let json_registry = state.json_schema_registry().clone();

      Ok((
        trailbase_sqlite::Connection::new(
          move || {
            return trailbase_extension::connect_sqlite(
              Some(db_path.clone()),
              Some(json_registry.clone()),
            );
          },
          None,
        )
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?,
        migration_path,
      ))
    }
    _ => Ok((
      state.conn().clone(),
      (state.data_dir().migrations_path().join("main")),
    )),
  };
}

async fn build_connection_metadata(
  state: &crate::AppState,
  conn: &trailbase_sqlite::Connection,
) -> Result<trailbase_schema::metadata::ConnectionMetadata, crate::schema_metadata::SchemaLookupError>
{
  use crate::schema_metadata::*;
  let tables = lookup_and_parse_all_table_schemas(conn).await?;
  let views = lookup_and_parse_all_view_schemas(conn, &tables).await?;

  return Ok(ConnectionMetadata::from_schemas(
    tables,
    views,
    &state.json_schema_registry().read(),
  )?);
}
