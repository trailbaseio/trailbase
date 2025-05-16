use serde::de::{Deserialize, Unexpected};

#[inline]
pub(crate) fn sanitize_column_name(name: &str) -> bool {
  // Assuming that all uses are quoted correctly, it should be enough to discard names containing
  // (", ', `, [, ]), however we're conservative here.
  return name
    .chars()
    .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_');
}

#[inline]
pub(crate) fn parse_bool(s: &str) -> Option<bool> {
  return match s {
    "TRUE" | "true" => Some(true),
    "FALSE" | "false" => Some(false),
    _ => None,
  };
}

pub(crate) fn deserialize_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
  D: serde::de::Deserializer<'de>,
{
  use serde::de::Error;
  use serde_value::Value;

  let value = Value::deserialize(deserializer)?;
  match value {
    Value::String(ref str) => {
      if let Some(b) = parse_bool(str) {
        return Ok(Some(b));
      }
    }
    Value::Bool(b) => {
      return Ok(Some(b));
    }
    _ => {}
  };

  return Err(Error::invalid_type(
    crate::util::unexpected(&value),
    &"'true' | 'TRUE' | 'false' | 'FALSE'",
  ));
}

pub(crate) fn unexpected(value: &serde_value::Value) -> Unexpected {
  use serde_value::Value;

  match *value {
    Value::Bool(b) => Unexpected::Bool(b),
    Value::U8(n) => Unexpected::Unsigned(n as u64),
    Value::U16(n) => Unexpected::Unsigned(n as u64),
    Value::U32(n) => Unexpected::Unsigned(n as u64),
    Value::U64(n) => Unexpected::Unsigned(n),
    Value::I8(n) => Unexpected::Signed(n as i64),
    Value::I16(n) => Unexpected::Signed(n as i64),
    Value::I32(n) => Unexpected::Signed(n as i64),
    Value::I64(n) => Unexpected::Signed(n),
    Value::F32(n) => Unexpected::Float(n as f64),
    Value::F64(n) => Unexpected::Float(n),
    Value::Char(c) => Unexpected::Char(c),
    Value::String(ref s) => Unexpected::Str(s),
    Value::Unit => Unexpected::Unit,
    Value::Option(_) => Unexpected::Option,
    Value::Newtype(_) => Unexpected::NewtypeStruct,
    Value::Seq(_) => Unexpected::Seq,
    Value::Map(_) => Unexpected::Map,
    Value::Bytes(ref b) => Unexpected::Bytes(b),
  }
}
