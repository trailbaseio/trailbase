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
        // TODO: We should probably switch to OID comparisons everywhere.
        match ty.name() {
          "bool" => (*v > 0).to_sql(ty, out)?,
          "char" => i8::try_from(*v)?.to_sql(ty, out)?,
          "int2" => i16::try_from(*v)?.to_sql(ty, out)?,
          "int4" => i32::try_from(*v)?.to_sql(ty, out)?,
          "tid" => {
            // NOTE: `tid`s in PG are a tuple like:
            //   struct Tid { pub block: u32, pub offset: u16, }
            let t: &[u8] = &v.to_be_bytes()[0..6];
            t.to_sql(ty, out)?
          }
          // NOTE: float8 is implicitly supported by the default below. This is just for symmetry.
          "float4" => (*v as f32).to_sql(ty, out)?,
          _ => v.to_sql(ty, out)?,
        };
      }
      Value::Real(v) => {
        match ty.name() {
          "float4" => (*v as f32).to_sql(ty, out)?,
          "float8" | _ => v.to_sql(ty, out)?,
        };
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
    return accepts_impl(ty);
  }

  postgres::types::to_sql_checked!();
}

impl<'a> postgres::types::FromSql<'a> for Value {
  fn from_sql(
    ty: &postgres::types::Type,
    raw: &'a [u8],
  ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
    // TODO: We should probably switch to OID comparisons everywhere.
    return match ty.name() {
      "bool" => Ok(Value::Integer(bool::from_sql(ty, raw)? as i64)),
      "char" => Ok(Value::Integer(i8::from_sql(ty, raw)? as i64)),
      "int2" => Ok(Value::Integer(i16::from_sql(ty, raw)? as i64)),
      "int4" => Ok(Value::Integer(i32::from_sql(ty, raw)? as i64)),
      "int8" => Ok(Value::Integer(i64::from_sql(ty, raw)?)),
      "float4" => Ok(Value::Real(f32::from_sql(ty, raw)? as f64)),
      "float8" => Ok(Value::Real(f64::from_sql(ty, raw)?)),
      "text" | "varchar" | "name" => Ok(Value::Text(String::from_sql(ty, raw)?)),
      "bytea" | "uuid" => Ok(Value::Blob(Vec::<u8>::from_sql(ty, raw)?)),
      "tid" => {
        // NOTE: `tid`s in PG are a tuple like:
        //   struct Tid { pub block: u32, pub offset: u16, }
        if raw.len() != 6 {
          return Err("TIDs are expected to be 6 bytes".into());
        }
        Ok(Value::Integer(i64::from_be_bytes([
          raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], 0, 0,
        ])))
      }
      _ => Err(format!("Unsupported type: {ty}").into()),
    };
  }

  fn from_sql_null(
    _ty: &postgres::types::Type,
  ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
    return Ok(Value::Null);
  }

  fn accepts(ty: &postgres::types::Type) -> bool {
    return accepts_impl(ty);
  }
}

#[inline]
fn accepts_impl(ty: &postgres::types::Type) -> bool {
  if *ty.kind() != postgres::types::Kind::Simple {
    return false;
  }

  // TODO: We should probably switch to OID comparisons everywhere.
  return matches!(
    ty.name(),
    "bool"
      | "char"
      | "int2"
      | "int4"
      | "int8"
      | "tid"
      | "float8"
      | "float4"
      | "json"
      | "text"
      | "name"
      | "varchar"
      | "bytea"
      | "uuid"
  );
}
