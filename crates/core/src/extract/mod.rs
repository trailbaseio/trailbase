mod content_type;
mod either;
mod multipart;
mod raw_json;

pub mod ip;
pub mod protobuf;

pub use either::Either;
pub use raw_json::RawJson;

/// Signals whether the server as a GET "/" route. Useful for redirects after auth actions.
#[derive(Clone)]
pub struct HasRoot(pub bool);
