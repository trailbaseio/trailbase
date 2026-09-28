use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
pub enum Error {
  #[error("JSONSchema validation error: {0}")]
  JsonSchema(Arc<jsonschema::ValidationError<'static>>),
  #[error("Cannot update builtin schemas")]
  BuiltinSchema,
  #[error("Missing name")]
  MissingName,
}
