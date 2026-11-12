use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("JSONSchema validation: {0}")]
  JsonSchema(Arc<jsonschema::ValidationError<'static>>),
  #[error("Cannot update builtin schemas")]
  BuiltinSchema,
  #[error("Missing name")]
  MissingName,
  #[error("NotImplemented")]
  NotImplemented,
  #[error("Other")]
  Other(Box<dyn std::error::Error + Send + Sync>),
}
