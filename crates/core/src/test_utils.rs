#[cfg(feature = "pg")]
pub fn conditionally_transform_query(sql: impl AsRef<str>) -> String {
  return sql.as_ref().replace("STRICT", "").replace("BLOB", "UUID");
}

#[cfg(not(feature = "pg"))]
pub fn conditionally_transform_query(sql: impl AsRef<str>) -> String {
  return sql.as_ref().to_string();
}
