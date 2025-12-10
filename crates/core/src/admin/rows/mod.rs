mod delete_rows;
mod insert_row;
mod list_rows;
mod read_files;
mod update_row;

pub(super) use delete_rows::{delete_row, delete_row_handler, delete_rows_handler};
pub(super) use insert_row::insert_row_handler;
pub(super) use list_rows::list_rows_handler;
pub(super) use read_files::read_files_handler;
pub(super) use update_row::update_row_handler;

fn build_connection(
  state: &crate::AppState,
  name: &trailbase_schema::QualifiedName,
) -> Result<std::sync::Arc<trailbase_sqlite::Connection>, crate::connection::ConnectionError> {
  if let Some(ref db) = name.database_schema
    && db != "main"
  {
    return state
      .connection_manager()
      .get(false, Some([db.to_string()].into()));
  }

  return state.connection_manager().get(true, None);
}
