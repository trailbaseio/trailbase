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
pub(crate) use create_table::{create_table_handler, CreateTableRequest};
pub(crate) use drop_table::drop_table_handler;

// Lists both Tables and Indexes
mod list_tables;

pub(crate) use list_tables::list_tables_handler;
