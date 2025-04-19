use itertools::Itertools;
use trailbase_schema::sqlite::ColumnOption;

use crate::config::{ConfigError, proto};
use crate::records::record_api::validate_rule;
use crate::table_metadata::{TableMetadataCache, TableOrViewMetadata};

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

  let metadata: std::sync::Arc<dyn TableOrViewMetadata> =
    if let Some(metadata) = tables.get(table_name) {
      if metadata.schema.temporary {
        return ierr("Record APIs must not reference TEMPORARY tables");
      }

      metadata
    } else if let Some(metadata) = tables.get_view(table_name) {
      if metadata.schema.temporary {
        return ierr("Record APIs must not reference TEMPORARY views");
      }

      metadata
    } else {
      return ierr(&format!("Missing table or view for API: {name}"));
    };

  if metadata.record_pk_column().is_none() {
    return ierr(&format!(
      "Table for api '{name}' is missing valid integer/uuidv7 primary key column."
    ));
  }

  let Some(columns) = metadata.columns() else {
    return ierr(&format!(
      "View for api '{name}' is not a \"simple\" view, i.e unable to infer types for strong type-safety"
    ));
  };

  for expand in &api_config.expand {
    if expand.starts_with("_") {
      return ierr(&format!("{name} expands hidden column: {expand}"));
    }

    let Some(column) = columns.iter().find(|c| c.name == *expand) else {
      return ierr(&format!("{name} expands missing column: {expand}"));
    };

    let Some(ColumnOption::ForeignKey {
      foreign_table: foreign_table_name,
      referred_columns,
      ..
    }) = column
      .options
      .iter()
      .find_or_first(|o| matches!(o, ColumnOption::ForeignKey { .. }))
    else {
      return ierr(&format!("{name} expands non-foreign-key column: {expand}"));
    };

    if foreign_table_name.starts_with("_") {
      return ierr(&format!(
        "{name} expands reference '{expand}' to hidden table: {foreign_table_name}"
      ));
    }

    let Some(foreign_table) = tables.get(foreign_table_name) else {
      return ierr(&format!(
        "{name} reference missing table: {foreign_table_name}"
      ));
    };

    let Some((_idx, foreign_pk_column)) = foreign_table.record_pk_column() else {
      return ierr(&format!(
        "{name} references pk-less table: {foreign_table_name}"
      ));
    };

    match referred_columns.len() {
      0 => {}
      1 => {
        if referred_columns[0] != foreign_pk_column.name {
          return ierr(&format!(
            "{name}.{expand} expands non-primary-key reference"
          ));
        }
      }
      _ => {
        return ierr(&format!(
          "Composite keys cannot be expanded for {name}.{expand}"
        ));
      }
    };
  }

  let rules = [
    &api_config.create_access_rule,
    &api_config.read_access_rule,
    &api_config.update_access_rule,
    &api_config.delete_access_rule,
    &api_config.schema_access_rule,
  ];
  for rule in rules.into_iter().flatten() {
    validate_rule(rule).map_err(ConfigError::Invalid)?;
  }

  return Ok(name.to_owned());
}
