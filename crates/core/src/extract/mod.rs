mod content_type;
mod either;
mod multipart;

pub mod ip;
pub mod protobuf;

pub use either::Either;

/// Signals whether the server as a GET "/" route. Useful for redirects after auth actions.
#[derive(Clone)]
pub struct HasRoot(pub bool);
