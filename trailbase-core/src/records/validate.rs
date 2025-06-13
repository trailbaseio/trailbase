use itertools::Itertools;
use trailbase_schema::QualifiedName;
use trailbase_schema::sqlite::ColumnOption;

use crate::config::{ConfigError, proto};
use crate::records::record_api::validate_rule;
use crate::schema_metadata::{SchemaMetadataCache, TableOrViewMetadata};

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
  schemas: &SchemaMetadataCache,
  api_config: &proto::RecordApiConfig,
) -> Result<String, ConfigError> {
  let ierr = |msg: &str| Err(ConfigError::Invalid(msg.to_string()));

  let Some(ref api_name) = api_config.name else {
    return ierr("RecordApi config misses name.");
  };
  validate_record_api_name(api_name)?;

  let Some(ref table_name) = api_config.table_name else {
    return ierr("RecordApi config misses table name.");
  };

  let metadata: std::sync::Arc<dyn TableOrViewMetadata> = {
    let table_name = QualifiedName::parse(table_name)?;
    if let Some(metadata) = schemas.get_table(&table_name) {
      if metadata.schema.temporary {
        return ierr("Record APIs must not reference TEMPORARY tables");
      }

      metadata
    } else if let Some(metadata) = schemas.get_view(&table_name) {
      if metadata.schema.temporary {
        return ierr("Record APIs must not reference TEMPORARY views");
      }

      metadata
    } else {
      return ierr(&format!("Missing table or view for API: {api_name}"));
    }
  };

  let Some((pk_index, _)) = metadata.record_pk_column() else {
    return ierr(&format!(
      "Table for api '{api_name}' is missing valid integer/UUID primary key column."
    ));
  };

  let Some(columns) = metadata.columns() else {
    return ierr(&format!(
      "View for api '{api_name}' is not a \"simple\" view, i.e unable to infer types for strong type-safety"
    ));
  };

  for excluded_column_name in &api_config.excluded_columns {
    let Some(excluded_index) = columns
      .iter()
      .position(|col| col.name == *excluded_column_name)
    else {
      return ierr(&format!(
        "Excluded column '{excluded_column_name}' in API '{api_name}' not found.",
      ));
    };

    if excluded_index == pk_index {
      return ierr(&format!(
        "PK column '{excluded_column_name}' cannot be excluded from API '{api_name}'.",
      ));
    }

    let excluded_column = &columns[excluded_index];
    if excluded_column.is_not_null() && !excluded_column.has_default() {
      return ierr(&format!(
        "Cannot exclude column '{excluded_column_name}' from API '{api_name}', which is NOT NULL and w/o DEFAULT",
      ));
    }
  }

  for expand in &api_config.expand {
    if expand.starts_with("_") {
      return ierr(&format!("{api_name} expands hidden column: {expand}"));
    }

    let Some(column) = columns.iter().find(|c| c.name == *expand) else {
      return ierr(&format!("{api_name} expands missing column: {expand}"));
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
      return ierr(&format!(
        "{api_name} expands non-foreign-key column: {expand}"
      ));
    };

    if foreign_table_name.starts_with("_") {
      return ierr(&format!(
        "{api_name} expands reference '{expand}' to hidden table: {foreign_table_name}"
      ));
    }

    let Some(foreign_table) = schemas.get_table(&QualifiedName::parse(foreign_table_name)?) else {
      return ierr(&format!(
        "{api_name} reference missing table: {foreign_table_name}"
      ));
    };

    let Some((_idx, foreign_pk_column)) = foreign_table.record_pk_column() else {
      return ierr(&format!(
        "{api_name} references pk-less table: {foreign_table_name}"
      ));
    };

    match referred_columns.len() {
      0 => {}
      1 => {
        if referred_columns[0] != foreign_pk_column.name {
          return ierr(&format!(
            "{api_name}.{expand} expands non-primary-key reference"
          ));
        }
      }
      _ => {
        return ierr(&format!(
          "Composite keys cannot be expanded for {api_name}.{expand}"
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

  return Ok(api_name.to_owned());
}
