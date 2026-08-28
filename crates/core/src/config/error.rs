#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
  #[error("Decode: {0}")]
  Decode(#[from] prost::DecodeError),
  #[error("Parse: {0}")]
  Parse(#[from] prost_reflect::text_format::ParseError),
  #[error("ParseInt: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("ParseBool: {0}")]
  ParseBool(#[from] std::str::ParseBoolError),
  #[error("Validation: {0}")]
  Invalid(String),
  #[error("Update: {0}")]
  Update(String),
  #[error("IO: {0}")]
  IO(#[from] std::io::Error),
  #[error("Id: {0}")]
  Id(#[from] uuid::Error),
  #[error("Schema: {0}")]
  Schema(#[from] trailbase_schema::sqlite::SchemaError),
  #[error("Textproto: {0}")]
  Textproto(#[from] crate::config::textproto::Error),
}
