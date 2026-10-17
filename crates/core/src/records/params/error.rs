#[derive(Debug, thiserror::Error)]
pub enum ParamsError {
  #[error("Not a Number")]
  NotANumber,
  #[error("Column: {0}")]
  Column(&'static str),
  #[error("Value not found")]
  ValueNotFound,
  #[error("Column not found")]
  ColumnNotFound,
  #[error("Unexpected type: {0}, expected {1}")]
  UnexpectedType(&'static str, String),
  #[error("Decoding: {0}")]
  Base64Decode(base64::DecodeError),
  #[error("Nested object: {0}")]
  NestedObject(String),
  #[error("Nested array: {0}")]
  NestedArray(String),
  #[error("Parse int: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("Parse float: {0}")]
  ParseFloat(#[from] std::num::ParseFloatError),
  #[error("Json validation: {0}")]
  JsonValidation(#[from] crate::schema_metadata::JsonSchemaError),
  #[error("Json serialization: {0}")]
  JsonSerialization(#[from] serde_json::Error),
  #[error("JsonSchema: {0}")]
  JsonSchema(#[from] trailbase_schema::json_schema::Error),
  #[error("ObjectStore: {0}")]
  Storage(#[from] object_store::Error),
  #[error("SqlValueDecode: {0}")]
  SqlValueDecode(#[from] trailbase_sqlvalue::DecodeError),
  #[cfg(any(feature = "geos", feature = "geos-static"))]
  #[error("Geos: {0}")]
  Geos(#[from] geos::Error),
}

impl From<trailbase_schema::json::JsonError> for ParamsError {
  fn from(value: trailbase_schema::json::JsonError) -> Self {
    return match value {
      trailbase_schema::json::JsonError::Finite => Self::NotANumber,
      trailbase_schema::json::JsonError::ValueNotFound => Self::ValueNotFound,
      trailbase_schema::json::JsonError::ColumnMismatch => Self::Column("mismatch"),
      trailbase_schema::json::JsonError::NotSupported => Self::UnexpectedType("?", "?".to_string()),
      trailbase_schema::json::JsonError::Decode(err) => Self::Base64Decode(err),
      trailbase_schema::json::JsonError::UnexpectedType(expected, got) => {
        Self::UnexpectedType(expected, format!("{got:?}"))
      }
      trailbase_schema::json::JsonError::ParseInt(err) => Self::ParseInt(err),
      trailbase_schema::json::JsonError::ParseFloat(err) => Self::ParseFloat(err),
      trailbase_schema::json::JsonError::Serde(err) => Self::JsonSerialization(err),
      #[cfg(any(feature = "geos", feature = "geos-static"))]
      trailbase_schema::json::JsonError::Geos(err) => Self::Geos(err),
    };
  }
}
