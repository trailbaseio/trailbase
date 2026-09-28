#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

// This crate became a kitchen sink for three different kinds of "schemas": DB schemas (e.g.
// Sqlite), general JSON work (e.g. turning DB values into JSON records), and JSONSchema(TM) stuff.
pub mod db;
pub mod json;
pub mod json_schema;

// TODO: Clean up legacy re-exports after breaking up into 3 modules.
pub use db::sqlite::QualifiedName;
// pub use error::Error;
// pub use file::{FileUpload, FileUploadData, FileUploadInput, FileUploads};
