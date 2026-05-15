#[cfg(feature = "pg")]
pub fn conditionally_transform_query(sql: impl AsRef<str>) -> String {
  // HACK: We're just trying to mend incompatible queries. In the future we should probably just
  // define proper DB-specific queries.
  return sql
    .as_ref()
    .replace("STRICT", "")
    .replace("BLOB", "UUID")
    .replace("INTEGER PRIMARY KEY", "SERIAL PRIMARY KEY");
}

#[cfg(not(feature = "pg"))]
pub fn conditionally_transform_query(sql: impl AsRef<str>) -> String {
  return sql.as_ref().to_string();
}
