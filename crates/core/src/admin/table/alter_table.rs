use axum::extract::{Json, State};
use log::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use trailbase_schema::sqlite::{Column, QualifiedName, Table};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::config::proto::hash_config;
use crate::transaction::{TransactionLog, TransactionRecorder};

#[derive(Clone, Debug, Deserialize, TS)]
pub enum AlterTableOperation {
  RenameTableTo { name: String },
  AddColumn { column: Column },
  DropColumn { name: String },
  AlterColumn { name: String, column: Column },
}

/// Request for altering `TABLE` schema.
#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct AlterTableRequest {
  pub source_schema: Table,
  pub operations: Vec<AlterTableOperation>,

  pub dry_run: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct AlterTableResponse {
  pub sql: String,
}

/// Admin-only handler for altering `TABLE` schemas.
///
/// NOTE: SQLite has very limited alter table support. Thus, we always recreate the table and move
/// the data over, see https://sqlite.org/lang_altertable.html.
pub async fn alter_table_handler(
  State(state): State<AppState>,
  Json(request): Json<AlterTableRequest>,
) -> Result<Json<AlterTableResponse>, Error> {
  if state.demo_mode() {
    return Err(Error::Precondition("Disallowed in demo".into()));
  }

  let AlterTableRequest {
    source_schema: source_table_schema,
    operations,
    dry_run,
  } = request;

  let dry_run = dry_run.unwrap_or(false);
  let (conn, migration_path) =
    super::get_conn_and_migration_path(&state, source_table_schema.name.database_schema.clone())?;

  debug!("Alter table:\nsource: {source_table_schema:?}\nops: {operations:?}",);

  if operations.is_empty() {
    return Ok(Json(AlterTableResponse {
      sql: "".to_string(),
    }));
  }

  // Check that removing columns won't break record API configuration. Note that table renames
  // will be fixed up automatically later.
  check_column_removals_invalidating_config(&state, &source_table_schema, &operations)?;

  let TargetSchema {
    mut ephemeral_table_schema,
    ephemeral_table_rename,
    column_mapping,
  } = build_ephemeral_target_schema(&source_table_schema, operations)?;

  let target_table_name = ephemeral_table_rename
    .as_ref()
    .unwrap_or(&ephemeral_table_schema.name)
    .clone();

  let tx_log = {
    let unqualified_source_table_name = source_table_schema.name.name.clone();
    let unqualified_ephemeral_table_rename =
      ephemeral_table_rename.as_ref().map(|n| n.name.clone());

    let (source_columns, target_columns): (Vec<String>, Vec<String>) =
      column_mapping.into_iter().unzip();

    // Strip qualification.
    ephemeral_table_schema.name.database_schema = None;

    conn
      .call(
        move |conn| -> Result<Option<TransactionLog>, trailbase_sqlite::Error> {
          let mut tx = TransactionRecorder::new(conn)
            .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          tx.execute("PRAGMA foreign_keys = OFF", ())?;

          // Create new table
          let sql = ephemeral_table_schema.create_table_statement();
          tx.execute(&sql, ()).map_err(|err| {
            warn!("Failed creating ephemeral table, likely invalid operations: {sql}\n\t{err}");
            return err;
          })?;

          // Copy
          let unqualified_ephemeral_table_name = ephemeral_table_schema.name.name;
          let insert_data_query = format!(
            r#"
            INSERT INTO
              "{unqualified_ephemeral_table_name}" ({target_columns})
            SELECT
              {source_columns}
            FROM
              "{unqualified_source_table_name}"
          "#,
            source_columns = escape_and_join_column_names(&source_columns),
            target_columns = escape_and_join_column_names(&target_columns),
          );
          tx.execute(&insert_data_query, ())?;

          tx.execute(
            &format!("DROP TABLE \"{unqualified_source_table_name}\""),
            (),
          )?;

          if let Some(unqualified_target_name) = unqualified_ephemeral_table_rename {
            // NOTE: w/o the `legacy_alter_table = ON` the following `RENAME TO` would fail, since
            // `ALTER TABLE` otherwise does a schema consistency-check and realize that any views
            // referencing this table are no longer valid (even though may be again after the
            // rename).
            tx.execute("PRAGMA legacy_alter_table = ON", ())?;
            tx.execute(
              &format!(
                "ALTER TABLE \"{unqualified_ephemeral_table_name}\" RENAME TO \"{unqualified_target_name}\""
              ),
              (),
            )?;
            tx.execute("PRAGMA legacy_alter_table = OFF", ())?;
          }

          tx.execute("PRAGMA foreign_keys = ON", ())?;

          return tx
            .rollback()
            .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
        },
      )
      .await?
  };

  // Take transaction log, write a migration file and apply.
  if !dry_run && let Some(ref log) = tx_log {
    let filename = QualifiedName {
      name: target_table_name.name.clone(),
      database_schema: None,
    }
    .migration_filename("alter_table");

    let report = log
      .apply_as_migration(&conn, migration_path, &filename)
      .await?;
    debug!("Migration report: {report:?}");

    // Fix configuration: update all table references by existing APIs.
    if source_table_schema.name != target_table_name {
      let mut config = state.get_config();
      let old_config_hash = hash_config(&config);

      for api in &mut config.record_apis {
        if let Some(ref name) = api.table_name
          && QualifiedName::parse(name)? == source_table_schema.name
        {
          if let Some(ref db) = target_table_name.database_schema {
            api.table_name = Some(format!("{}.{}", db, target_table_name.name));
          } else {
            api.table_name = Some(target_table_name.name.clone());
          }
        }
      }

      state
        .validate_and_update_config(config, Some(old_config_hash))
        .await?;
    }

    state.rebuild_connection_metadata().await?;
  }

  return Ok(Json(AlterTableResponse {
    sql: tx_log.map(|l| l.build_sql()).unwrap_or_default(),
  }));
}

struct TargetSchema {
  ephemeral_table_schema: Table,
  ephemeral_table_rename: Option<QualifiedName>,
  column_mapping: HashMap<String, String>,
}

// Returns the (ephemeral) target schema + a rename if necessary, i.e. if the ephemeral and the
// ultimate target schema are not the same.
fn build_ephemeral_target_schema(
  source_schema: &Table,
  operations: Vec<AlterTableOperation>,
) -> Result<TargetSchema, Error> {
  // QUESTION: Should we respect operation order or sort them, e.g. drops, then alters, then
  // additions? The should be correct by construction :shrug:
  let mut needs_rename: Option<QualifiedName> = Some(source_schema.name.clone());
  let mut column_mapping = HashMap::<String, String>::from_iter(
    source_schema
      .columns
      .iter()
      .map(|c| (c.name.clone(), c.name.clone())),
  );

  let mut schema = {
    let mut schema = source_schema.clone();
    schema.name = QualifiedName {
      name: format!("__alter_table_{}", source_schema.name.name),
      database_schema: source_schema.name.database_schema.clone(),
    };
    schema
  };

  for operation in operations {
    match operation {
      AlterTableOperation::RenameTableTo { name } => {
        needs_rename = None;
        schema.name = QualifiedName {
          name,
          database_schema: source_schema.name.database_schema.clone(),
        };
      }
      AlterTableOperation::DropColumn { name } => {
        schema.columns.retain(|c| c.name != name);
        if column_mapping.remove(&name).is_none() {
          return Err(Error::BadRequest(format!("Column '{name}' missing").into()));
        }
      }
      AlterTableOperation::AlterColumn { name, column } => {
        let Some(pos) = schema.columns.iter().position(|c| c.name == name) else {
          return Err(Error::BadRequest(format!("Column '{name}' missing").into()));
        };

        if name != column.name {
          // Column rename.
          if column_mapping.contains_key(&column.name) {
            return Err(Error::BadRequest(
              format!("Column '{}' already exists", column.name).into(),
            ));
          }

          let res = column_mapping.insert(name.clone(), column.name.clone());
          assert_eq!(res, Some(name));
        }

        schema.columns[pos] = column;
      }
      AlterTableOperation::AddColumn { column } => {
        if column_mapping.contains_key(&column.name) {
          return Err(Error::BadRequest(
            format!("Column '{}' already exists", column.name).into(),
          ));
        }
        schema.columns.push(column);
      }
    }
  }

  return Ok(TargetSchema {
    ephemeral_table_schema: schema,
    ephemeral_table_rename: needs_rename,
    column_mapping,
  });
}

fn check_column_removals_invalidating_config(
  state: &AppState,
  source_schema: &Table,
  operations: &[AlterTableOperation],
) -> Result<(), Error> {
  // Check that removing columns won't break record API configuration.
  let deleted_columns: Vec<String> = operations
    .iter()
    .flat_map(|op| {
      if let AlterTableOperation::DropColumn { name } = op {
        return Some(name.clone());
      }
      return None;
    })
    .collect();

  if deleted_columns.is_empty() {
    return Ok(());
  }

  let config = state.get_config();
  for api in &config.record_apis {
    let api_name = api.name();
    let api_table = QualifiedName::parse(api.table_name.as_deref().unwrap_or_default())?;
    if api_table != source_schema.name {
      continue;
    }

    for expanded_column in &api.expand {
      if deleted_columns.contains(expanded_column) {
        return Err(Error::BadRequest(
          format!("Cannot remove column {expanded_column} referenced by API: {api_name}").into(),
        ));
      }
    }

    for excluded_column in &api.excluded_columns {
      if deleted_columns.contains(excluded_column) {
        return Err(Error::BadRequest(
          format!("Cannot remove column {excluded_column} referenced by API: {api_name}").into(),
        ));
      }
    }

    // Check that column is not referenced in rules.
    for rule in [
      &api.read_access_rule,
      &api.create_access_rule,
      &api.update_access_rule,
      &api.delete_access_rule,
      &api.schema_access_rule,
    ]
    .into_iter()
    .flatten()
    {
      for deleted_column in &deleted_columns {
        // NOTE: ideally, we'd parse the rule like in crate::records::record_api::validate_rule.
        // The current approach would fail if the column name is a keyword used as part of the rule
        // query. In the meantime, let's error on the side of false positive.
        const KEYWORDS: &[&str] = &[
          "select", "in", "where", "as", "and", "or", "is", "null", "coalesce",
        ];
        if KEYWORDS.contains(&deleted_column.to_lowercase().as_str()) {
          continue;
        }

        if rule.contains(deleted_column) {
          return Err(Error::BadRequest(
            format!("Cannot remove column {deleted_column} referenced by access rule: {rule}")
              .into(),
          ));
        }
      }
    }
  }

  return Ok(());
}

fn escape_and_join_column_names(names: &[String]) -> String {
  use itertools::Itertools;
  return names.iter().map(|n| format!("\"{n}\"")).join(", ");
}

#[cfg(test)]
mod tests {
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::{Column, ColumnAffinityType, ColumnDataType, ColumnOption, Table};

  use super::*;
  use crate::admin::table::{CreateTableRequest, create_table_handler};
  use crate::app_state::*;

  fn parse_create_table(create_table_sql: &str) -> Table {
    let create_table_statement = parse_into_statement(create_table_sql).unwrap().unwrap();
    return create_table_statement.try_into().unwrap();
  }

  #[test]
  fn test_target_schema_construction() {
    let source_schema = parse_create_table(
      "
        CREATE TABLE test (
            id    INTEGER PRIMARY KEY,
            a     TEXT,
            b     TEXT NOT NULL
        );
      ",
    );

    {
      // Table rename.
      let TargetSchema {
        ephemeral_table_schema,
        ephemeral_table_rename,
        column_mapping,
      } = build_ephemeral_target_schema(
        &source_schema,
        vec![AlterTableOperation::RenameTableTo {
          name: "rename".to_string(),
        }],
      )
      .unwrap();

      assert!(ephemeral_table_rename.is_none());
      assert_eq!("rename", ephemeral_table_schema.name.name);
      assert_eq!(3, column_mapping.len());

      for (source, target) in column_mapping {
        assert_eq!(source, target);
      }
    }

    {
      // Add/drop column
      let add_column = Column {
        name: "c".to_string(),
        type_name: "real".to_string(),
        data_type: ColumnDataType::Real,
        affinity_type: ColumnAffinityType::Real,
        options: vec![],
      };
      let TargetSchema {
        ephemeral_table_schema,
        ephemeral_table_rename,
        column_mapping,
      } = build_ephemeral_target_schema(
        &source_schema,
        vec![
          AlterTableOperation::DropColumn {
            name: "a".to_string(),
          },
          AlterTableOperation::DropColumn {
            name: "b".to_string(),
          },
          AlterTableOperation::AddColumn {
            column: add_column.clone(),
          },
        ],
      )
      .unwrap();

      assert_eq!(
        Some("test"),
        ephemeral_table_rename.as_ref().map(|qn| qn.name.as_str())
      );
      assert!(ephemeral_table_schema.name.name.starts_with("__"));
      // With "a" and "b" gone, only the id column has a before<->after mapping.
      assert_eq!(1, column_mapping.len());
      assert_eq!(Some(&"id".to_string()), column_mapping.get("id"));

      assert_eq!(2, ephemeral_table_schema.columns.len());
      assert_eq!(add_column, ephemeral_table_schema.columns[1]);
    }

    {
      // Alter column
      let renamed_column = Column {
        name: "renamed".to_string(),
        type_name: "TEXT".to_string(),
        data_type: ColumnDataType::Text,
        affinity_type: ColumnAffinityType::Text,
        options: vec![],
      };
      let TargetSchema {
        ephemeral_table_schema,
        ephemeral_table_rename,
        column_mapping,
      } = build_ephemeral_target_schema(
        &source_schema,
        vec![AlterTableOperation::AlterColumn {
          name: "a".to_string(),
          column: renamed_column.clone(),
        }],
      )
      .unwrap();

      assert_eq!(
        Some("test"),
        ephemeral_table_rename.as_ref().map(|qn| qn.name.as_str())
      );
      assert!(ephemeral_table_schema.name.name.starts_with("__"));
      // With "a" and "b" gone, only the id column has a before<->after mapping.
      assert_eq!(3, column_mapping.len());
      assert_eq!(Some(&"renamed".to_string()), column_mapping.get("a"));

      assert_eq!(3, ephemeral_table_schema.columns.len());
      assert_eq!(renamed_column, ephemeral_table_schema.columns[1]);
    }

    // Rename column to already existing one.
    assert!(
      build_ephemeral_target_schema(
        &source_schema,
        vec![AlterTableOperation::AlterColumn {
          name: "a".to_string(),
          column: Column {
            name: "b".to_string(),
            type_name: "text".to_string(),
            data_type: ColumnDataType::Text,
            affinity_type: ColumnAffinityType::Text,
            options: vec![],
          },
        }],
      )
      .is_err()
    );

    // Rename column twice.
    assert!(
      build_ephemeral_target_schema(
        &source_schema,
        vec![
          AlterTableOperation::AlterColumn {
            name: "a".to_string(),
            column: Column {
              name: "rename1".to_string(),
              type_name: "text".to_string(),
              data_type: ColumnDataType::Text,
              affinity_type: ColumnAffinityType::Text,
              options: vec![],
            },
          },
          AlterTableOperation::AlterColumn {
            name: "a".to_string(),
            column: Column {
              name: "rename2".to_string(),
              type_name: "text".to_string(),
              data_type: ColumnDataType::Text,
              affinity_type: ColumnAffinityType::Text,
              options: vec![],
            },
          }
        ],
      )
      .is_err()
    );
  }

  #[tokio::test]
  async fn test_alter_table() -> Result<(), anyhow::Error> {
    let state = test_state(None).await?;
    let conn = state.conn();
    let pk_col = "my_pk".to_string();

    let create_table_request = CreateTableRequest {
      schema: Table {
        name: QualifiedName::parse("foo").unwrap(),
        strict: true,
        columns: vec![Column {
          name: pk_col.clone(),
          type_name: "blob".to_string(),
          data_type: ColumnDataType::Blob,
          affinity_type: ColumnAffinityType::Blob,
          options: vec![ColumnOption::Unique {
            is_primary: true,
            conflict_clause: None,
          }],
        }],
        foreign_keys: vec![],
        unique: vec![],
        checks: vec![],
        virtual_table: false,
        temporary: false,
      },
      dry_run: Some(false),
    };
    debug!(
      "Create Table: {}",
      create_table_request.schema.create_table_statement()
    );
    let _ = create_table_handler(State(state.clone()), Json(create_table_request.clone())).await?;

    conn
      .read_query_rows(format!("SELECT {pk_col} FROM foo"), ())
      .await?;

    {
      // Noop: source and target identical.
      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        operations: vec![],
        dry_run: None,
      };

      let Json(response) =
        alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
          .await
          .unwrap();
      assert_eq!(response.sql, "");

      conn
        .read_query_rows(format!("SELECT {pk_col} FROM foo"), ())
        .await?;
    }

    {
      // Add column.
      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        operations: vec![AlterTableOperation::AddColumn {
          column: Column {
            name: "new".to_string(),
            type_name: "text".to_string(),
            data_type: ColumnDataType::Text,
            affinity_type: ColumnAffinityType::Text,
            options: vec![
              ColumnOption::NotNull,
              ColumnOption::Default("'default'".to_string()),
            ],
          },
        }],
        dry_run: None,
      };

      let Json(response) =
        alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
          .await
          .unwrap();
      assert!(response.sql.contains("new"));

      conn
        .read_query_rows(format!("SELECT {pk_col}, new FROM foo"), ())
        .await?;
    }

    {
      // Rename table and remove "new" column.
      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        operations: vec![AlterTableOperation::RenameTableTo {
          name: "bar".to_string(),
        }],
        dry_run: None,
      };

      let Json(response) =
        alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
          .await
          .unwrap();
      assert!(response.sql.contains("bar"));

      assert!(conn.read_query_rows("SELECT * FROM foo", ()).await.is_err());
      conn
        .read_query_rows(format!("SELECT {pk_col} FROM bar"), ())
        .await?;
    }

    return Ok(());
  }
}
