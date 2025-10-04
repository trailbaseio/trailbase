use itertools::Itertools;
use trailbase_schema::QualifiedName;
use trailbase_schema::parse::parse_into_statement;
use trailbase_schema::sqlite::ColumnOption;

use crate::config::{ConfigError, proto};
use crate::schema_metadata::{SchemaMetadataCache, TableOrViewMetadata};

fn validate_record_api_name(name: &str) -> Result<(), ConfigError> {
  if name.is_empty() {
    return Err(invalid("Invalid api name: cannot be empty"));
  }

  if !name.chars().all(|x| x.is_ascii_alphanumeric() || x == '_') {
    return Err(invalid(format!(
      "Invalid api name: {name}. Must only contain alphanumeric characters or '_'."
    )));
  }

  Ok(())
}

pub(crate) fn validate_record_api_config(
  schemas: &SchemaMetadataCache,
  api_config: &proto::RecordApiConfig,
) -> Result<String, ConfigError> {
  let Some(ref api_name) = api_config.name else {
    return Err(invalid("RecordApi config misses name."));
  };
  validate_record_api_name(api_name)?;

  let Some(ref table_name) = api_config.table_name else {
    return Err(invalid("RecordApi config misses table name."));
  };

  let metadata: std::sync::Arc<dyn TableOrViewMetadata> = {
    let table_name = QualifiedName::parse(table_name)?;
    if let Some(metadata) = schemas.get_table(&table_name) {
      if metadata.schema.temporary {
        return Err(invalid("Record APIs must not reference TEMPORARY tables"));
      }

      metadata
    } else if let Some(metadata) = schemas.get_view(&table_name) {
      if metadata.schema.temporary {
        return Err(invalid("Record APIs must not reference TEMPORARY views"));
      }

      metadata
    } else {
      return Err(invalid(format!(
        "Missing table or view for API: {api_name}"
      )));
    }
  };

  let Some((pk_index, _)) = metadata.record_pk_column() else {
    return Err(invalid(format!(
      "Table for api '{api_name}' is missing valid integer/UUID primary key column."
    )));
  };

  let Some(columns) = metadata.columns() else {
    return Err(invalid(format!(
      "View for api '{api_name}' is not a \"simple\" view, i.e unable to infer types for strong type-safety"
    )));
  };

  for excluded_column_name in &api_config.excluded_columns {
    let Some(excluded_index) = columns
      .iter()
      .position(|col| col.name == *excluded_column_name)
    else {
      return Err(invalid(format!(
        "Excluded column '{excluded_column_name}' in API '{api_name}' not found.",
      )));
    };

    if excluded_index == pk_index {
      return Err(invalid(format!(
        "PK column '{excluded_column_name}' cannot be excluded from API '{api_name}'.",
      )));
    }

    let excluded_column = &columns[excluded_index];
    if excluded_column.is_not_null() && !excluded_column.has_default() {
      return Err(invalid(format!(
        "Cannot exclude column '{excluded_column_name}' from API '{api_name}', which is NOT NULL and w/o DEFAULT",
      )));
    }
  }

  for expand in &api_config.expand {
    if expand.starts_with("_") {
      return Err(invalid(format!(
        "{api_name} expands hidden column: {expand}"
      )));
    }

    let Some(column) = columns.iter().find(|c| c.name == *expand) else {
      return Err(invalid(format!(
        "{api_name} expands missing column: {expand}"
      )));
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
      return Err(invalid(format!(
        "{api_name} expands non-foreign-key column: {expand}"
      )));
    };

    if foreign_table_name.starts_with("_") {
      return Err(invalid(format!(
        "{api_name} expands reference '{expand}' to hidden table: {foreign_table_name}"
      )));
    }

    let Some(foreign_table) = schemas.get_table(&QualifiedName::parse(foreign_table_name)?) else {
      return Err(invalid(format!(
        "{api_name} reference missing table: {foreign_table_name}"
      )));
    };

    let Some((_idx, foreign_pk_column)) = foreign_table.record_pk_column() else {
      return Err(invalid(format!(
        "{api_name} references pk-less table: {foreign_table_name}"
      )));
    };

    match referred_columns.len() {
      0 => {}
      1 => {
        if referred_columns[0] != foreign_pk_column.name {
          return Err(invalid(format!(
            "{api_name}.{expand} expands non-primary-key reference"
          )));
        }
      }
      _ => {
        return Err(invalid(format!(
          "Composite keys cannot be expanded for {api_name}.{expand}"
        )));
      }
    };
  }

  let rules = [
    (AccessKind::Create, api_config.create_access_rule.as_ref()),
    (AccessKind::Read, api_config.read_access_rule.as_ref()),
    (AccessKind::Update, api_config.update_access_rule.as_ref()),
    (AccessKind::Delete, api_config.delete_access_rule.as_ref()),
    (AccessKind::Schema, api_config.schema_access_rule.as_ref()),
  ];
  for (kind, rule) in rules {
    if let Some(rule) = rule {
      validate_rule(kind, rule).map_err(invalid)?;
    }
  }

  return Ok(api_name.to_owned());
}

enum AccessKind {
  Create,
  Read,
  Update,
  Delete,
  Schema,
}

fn validate_rule(kind: AccessKind, rule: &str) -> Result<(), ConfigError> {
  for magic in ["_USER_", "_REQ_", "_REQ_FIELDS_", "_ROW_"] {
    if rule.contains(&magic.to_lowercase()) {
      return Err(invalid(
        "Access rule '{rule}', contained lower-case {magic}, upper-case expected",
      ));
    }
  }

  // NOTE: We could probably do this as part of the recursive AST traversal below rather than
  // string match.
  // We may also want to scan more actively for typos... , e.g. _ROW_ vs _row_.
  match kind {
    AccessKind::Create => {
      if rule.contains("_ROW_") {
        return Err(invalid("Create rule cannot reference _ROW_"));
      }
    }
    AccessKind::Read => {
      if rule.contains("_REQ_") || rule.contains("_REQ_FIELDS_") {
        return Err(invalid("Read rule cannot reference _REQ_"));
      }
    }
    AccessKind::Update => {}
    AccessKind::Delete => {
      if rule.contains("_REQ_") || rule.contains("_REQ_FIELDS_") {
        return Err(invalid("Delete rule cannot reference _REQ_"));
      }
    }
    AccessKind::Schema => {
      if rule.contains("_ROW_") {
        return Err(invalid("Schema rule cannot reference _ROW_"));
      }
      if rule.contains("_REQ_") || rule.contains("_REQ_FIELDS_") {
        return Err(invalid("Schema rule cannot reference _REQ_"));
      }
    }
  }

  let stmt = parse_into_statement(&format!("SELECT {rule}"))
    .map_err(|err| invalid(format!("'{rule}' not a valid SQL expression: {err}")))?;

  let Some(sqlite3_parser::ast::Stmt::Select(select)) = stmt else {
    return Err(invalid(format!(
      "Access rule '{rule}' not a select statement"
    )));
  };

  let sqlite3_parser::ast::OneSelect::Select { mut columns, .. } = select.body.select else {
    return Err(invalid(format!(
      "Access rule '{rule}' not a select statement"
    )));
  };

  if columns.len() != 1 {
    return Err(invalid("Expected single column"));
  }

  let sqlite3_parser::ast::ResultColumn::Expr(expr, _) = columns.swap_remove(0) else {
    return Err(invalid("Expected expr"));
  };

  validate_expr_recursively(&expr)?;

  return Ok(());
}

fn validate_expr_recursively(expr: &sqlite3_parser::ast::Expr) -> Result<(), ConfigError> {
  use sqlite3_parser::ast;

  match &expr {
    ast::Expr::Binary(lhs, _op, rhs) => {
      validate_expr_recursively(lhs)?;
      validate_expr_recursively(rhs)?;
    }
    ast::Expr::IsNull(inner) => {
      validate_expr_recursively(inner)?;
    }
    // Ensure `IN _REQ_FIELDS_` expression are preceded by literals, e.g.:
    //   `'field' IN _REQ_FIELDS_`.
    ast::Expr::InTable { lhs, rhs, .. } => {
      match rhs {
        ast::QualifiedName {
          name: ast::Name(name),
          ..
        } if name.as_ref() == "_REQ_FIELDS_" => {
          if !matches!(**lhs, ast::Expr::Literal(ast::Literal::String(_))) {
            return Err(invalid(format!(
              "Expected literal string on LHS of `IN _REQ_FIELDS_`, got: {lhs:?}"
            )));
          }
        }
        _ => {}
      };

      validate_expr_recursively(lhs)?;
    }
    _ => {}
  }

  return Ok(());
}

fn invalid(err: impl std::string::ToString) -> ConfigError {
  return ConfigError::Invalid(err.to_string());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_validate_rule() {
    assert!(validate_rule(AccessKind::Read, "").is_err());
    assert!(validate_rule(AccessKind::Read, "1, 1").is_err());
    assert!(validate_rule(AccessKind::Read, "1").is_ok());

    validate_rule(AccessKind::Read, "_USER_.id IS NOT NULL").unwrap();
    validate_rule(
      AccessKind::Read,
      "_USER_.id IS NOT NULL AND _ROW_.userid = _USER_.id",
    )
    .unwrap();

    assert!(validate_rule(AccessKind::Read, "_REQ_.field = 'magic'").is_err());

    validate_rule(
      AccessKind::Create,
      "_USER_.id IS NOT NULL AND _REQ_.field IS NOT NULL",
    )
    .unwrap();

    assert!(validate_rule(AccessKind::Update, "'field' IN _REQ_FIELDS_").is_ok());
    assert!(validate_rule(AccessKind::Update, "field IN _REQ_FIELDS_").is_err());
  }
}
