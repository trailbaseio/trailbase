use crate::value::Value;

impl postgres::types::ToSql for Value {
  fn to_sql(
    &self,
    ty: &postgres::types::Type,
    out: &mut bytes::BytesMut,
  ) -> Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
  where
    Self: Sized,
  {
    match self {
      Value::Null => return Ok(postgres::types::IsNull::Yes),
      Value::Integer(v) => {
        v.to_sql(ty, out)?;
      }
      Value::Real(v) => {
        v.to_sql(ty, out)?;
      }
      Value::Text(v) => {
        v.to_sql(ty, out)?;
      }
      Value::Blob(v) => {
        v.to_sql(ty, out)?;
      }
    };
    return Ok(postgres::types::IsNull::No);
  }

  /// Determines if a value of this type can be converted to the specified
  /// Postgres `Type`.
  fn accepts(ty: &postgres::types::Type) -> bool
  where
    Self: Sized,
  {
    if *ty.kind() != postgres::types::Kind::Simple {
      return false;
    }

    // TODO: further validate based on `ty.oid()`?.
    return true;
  }

  postgres::types::to_sql_checked!();
}

impl<'a> postgres::types::FromSql<'a> for Value {
  fn from_sql(
    ty: &postgres::types::Type,
    raw: &'a [u8],
  ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
    return match ty.name() {
      "int8" => Ok(Value::Integer(i64::from_sql(ty, raw)?)),
      "int4" => Ok(Value::Integer(i32::from_sql(ty, raw)? as i64)),
      "float8" => Ok(Value::Real(f64::from_sql(ty, raw)?)),
      "float4" => Ok(Value::Real(f32::from_sql(ty, raw)? as f64)),
      "text" | "varchar" => Ok(Value::Text(String::from_sql(ty, raw)?)),
      "bytea" => Ok(Value::Blob(Vec::<u8>::from_sql(ty, raw)?)),
      _ => Err(format!("Unsupported type: {ty}").into()),
    };
  }

  fn from_sql_null(
    _ty: &postgres::types::Type,
  ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
    return Ok(Value::Null);
  }

  fn accepts(ty: &postgres::types::Type) -> bool {
    return matches!(
      ty.name(),
      "int8" | "int4" | "float8" | "float4" | "text" | "varchar" | "bytea"
    );
  }
}
