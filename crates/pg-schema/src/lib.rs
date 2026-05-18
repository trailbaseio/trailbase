#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod error;
mod index;
mod table;
mod trigger;
mod util;
mod view;

pub use crate::error::Error;
pub use crate::index::build_all_index_schemas;
pub use crate::table::build_all_table_schemas;
pub use crate::trigger::{TriggerInformationSchema, build_all_trigger_schemas};
pub use crate::view::build_all_view_schemas;
