use crate::config::{proto, ConfigError};
use crate::table_metadata::{
  sqlite3_parse_into_statements, TableMetadataCache, TableOrViewMetadata,
};

fn validate_record_api_name(name: &str) -> Result<(), ConfigError> {
  if name.is_empty() {
    return Err(ConfigError::Invalid(
      "Invalid api name: cannot be empty".to_string(),
    ));
  }

  if !name.chars().all(|x| x.is_ascii_alphanumeric() || x == '_') {
    return Err(ConfigError::Invalid(format!(
      "Invalid api name: {name}. Must only contain alphanumeric characters or '_'."
    )));
  }

  Ok(())
}

pub(crate) fn validate_record_api_config(
  tables: &TableMetadataCache,
  api_config: &proto::RecordApiConfig,
) -> Result<String, ConfigError> {
  let ierr = |msg: &str| Err(ConfigError::Invalid(msg.to_string()));

  let Some(ref name) = api_config.name else {
    return ierr("RecordApi config misses name.");
  };
  validate_record_api_name(name)?;

  let Some(ref table_name) = api_config.table_name else {
    return ierr("RecordApi config misses table name.");
  };

  if let Some(metadata) = tables.get(table_name) {
    if !metadata.schema.strict {
      return Err(ConfigError::Invalid(format!(
        "RecordApi table '{table_name}' for api '{name}' must be strict to support JSON schema and type-safety."
      )));
    }

    if metadata.record_pk_column().is_none() {
      return Err(ConfigError::Invalid(format!(
        "Table for api '{name}' is missing valid integer/uuidv7 primary key column: {:?}",
        metadata.schema
      )));
    }
  } else if let Some(metadata) = tables.get_view(table_name) {
    if metadata.schema.temporary {
      return Err(ConfigError::Invalid(format!(
        "RecordApi {name} references temporary view: {table_name}"
      )));
    }

    if metadata.record_pk_column().is_none() {
      return Err(ConfigError::Invalid(format!(
        "View for api '{name}' is missing valid integer/uuidv7 primary key column: {:?}",
        metadata.schema
      )));
    }

    let Some(ref _columns) = metadata.schema.columns else {
      return Err(ConfigError::Invalid(format!(
        "View for api '{name}' is not a \"simple\" view, i.e. the column types couldn't be inferred and thus type-safety cannot be guaranteed."
      )));
    };
  } else {
    return Err(ConfigError::Invalid(format!(
      "Missing table or view for API: {name}"
    )));
  }

  let rules = [
    &api_config.create_access_rule,
    &api_config.read_access_rule,
    &api_config.update_access_rule,
    &api_config.delete_access_rule,
    &api_config.schema_access_rule,
  ];
  for rule in rules.into_iter().flatten() {
    let map = |err| ConfigError::Invalid(format!("'{rule}' not a valid SQL expression: {err}"));

    // const DIALECT: SQLiteDialect = SQLiteDialect {};
    // SqlParser::new(&DIALECT)
    //   .try_with_sql(rule)
    //   .map_err(map)?
    //   .parse_expr()
    //   .map_err(map)?;

    let _statements = sqlite3_parse_into_statements(&format!("SELECT ({rule})")).map_err(map)?;
  }

  return Ok(name.clone());
}
