#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod error;
pub mod file;
pub mod registry;
pub mod sqlite;

pub use error::Error;
pub use file::{FileUpload, FileUploadInput, FileUploads};
