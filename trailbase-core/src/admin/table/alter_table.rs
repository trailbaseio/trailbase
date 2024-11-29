use std::collections::HashSet;

use axum::{
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use log::*;
use serde::Deserialize;
use ts_rs::TS;

use crate::app_state::AppState;
use crate::schema::Table;
use crate::transaction::TransactionRecorder;
use crate::{admin::AdminError as Error, transaction::MigrationWriter};

#[derive(Clone, Debug, Deserialize, TS)]
#[ts(export)]
pub struct AlterTableRequest {
  pub source_schema: Table,
  pub target_schema: Table,
}

// NOTE: sqlite has very limited alter table support, thus we're always recreating the table and
// moving data over, see https://sqlite.org/lang_altertable.html.

pub async fn alter_table_handler(
  State(state): State<AppState>,
  Json(request): Json<AlterTableRequest>,
) -> Result<Response, Error> {
  let source_schema = request.source_schema;
  let source_table_name = source_schema.name.clone();

  let Some(_metadata) = state.table_metadata().get(&source_table_name) else {
    return Err(Error::Precondition(format!(
      "Cannot alter '{source_table_name}'. Only tables are supported.",
    )));
  };

  let target_schema = request.target_schema;
  let target_table_name = target_schema.name.clone();

  debug!("Alter table:\nsource: {source_schema:?}\ntarget: {target_schema:?}",);

  let temp_table_name: String = {
    if target_table_name != source_table_name {
      target_table_name.clone()
    } else {
      format!("__alter_table_{target_table_name}")
    }
  };

  let source_columns: HashSet<String> = source_schema
    .columns
    .iter()
    .map(|c| c.name.clone())
    .collect();
  let copy_columns: Vec<String> = target_schema
    .columns
    .iter()
    .filter_map(|c| {
      if source_columns.contains(&c.name) {
        Some(c.name.clone())
      } else {
        None
      }
    })
    .collect();

  let mut target_schema_copy = target_schema.clone();
  target_schema_copy.name = temp_table_name.to_string();

  let migration_path = state.data_dir().migrations_path();
  let conn = state.conn();
  let writer = conn
    .call(
      move |conn| -> Result<Option<MigrationWriter>, tokio_rusqlite::Error> {
        let mut tx = TransactionRecorder::new(
          conn,
          migration_path,
          format!("alter_table_{source_table_name}"),
        )
        .map_err(|err| tokio_rusqlite::Error::Other(err.into()))?;

        tx.execute("PRAGMA foreign_keys = OFF")?;

        // Create new table
        let sql = target_schema_copy.create_table_statement();
        tx.execute(&sql)?;

        // Copy
        tx.execute(&format!(
          r#"
            INSERT INTO
              {temp_table_name} ({column_list})
            SELECT
              {column_list}
            FROM
              {source_table_name}
          "#,
          column_list = copy_columns.join(", "),
        ))?;

        tx.execute(&format!("DROP TABLE {source_table_name}"))?;

        if *target_table_name != temp_table_name {
          tx.execute(&format!(
            "ALTER TABLE '{temp_table_name}' RENAME TO '{target_table_name}'"
          ))?;
        }

        tx.execute("PRAGMA foreign_keys = ON")?;

        return tx
          .rollback_and_create_migration()
          .map_err(|err| tokio_rusqlite::Error::Other(err.into()));
      },
    )
    .await?;

  // Write to migration file.
  if let Some(writer) = writer {
    let report = writer.write(conn).await?;
    debug!("Migration report: {report:?}");
  }

  state.table_metadata().invalidate_all().await?;

  return Ok((StatusCode::OK, "altered table").into_response());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::admin::table::{create_table_handler, CreateTableRequest};
  use crate::app_state::*;
  use crate::schema::{Column, ColumnDataType, ColumnOption, Table};

  #[tokio::test]
  async fn test_alter_table() -> Result<(), anyhow::Error> {
    let state = test_state(None).await?;
    let conn = state.conn();
    let pk_col = "my_pk".to_string();

    let create_table_request = CreateTableRequest {
      schema: Table {
        name: "foo".to_string(),
        strict: true,
        columns: vec![Column {
          name: pk_col.clone(),
          data_type: ColumnDataType::Blob,
          options: vec![ColumnOption::Unique { is_primary: true }],
        }],
        foreign_keys: vec![],
        unique: vec![],
        virtual_table: false,
        temporary: false,
      },
      dry_run: Some(false),
    };
    info!(
      "Create Table: {}",
      create_table_request.schema.create_table_statement()
    );
    let _ = create_table_handler(State(state.clone()), Json(create_table_request.clone())).await?;

    conn.query(&format!("SELECT {pk_col} FROM foo"), ()).await?;

    {
      // Noop: source and target identical.
      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        target_schema: create_table_request.schema.clone(),
      };

      alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
        .await
        .unwrap();

      conn.query(&format!("SELECT {pk_col} FROM foo"), ()).await?;
    }

    {
      // Add column.
      let mut target_schema = create_table_request.schema.clone();

      target_schema.columns.push(Column {
        name: "new".to_string(),
        data_type: ColumnDataType::Text,
        options: vec![
          ColumnOption::NotNull,
          ColumnOption::Default("'default'".to_string()),
        ],
      });

      info!("{}", target_schema.create_table_statement());

      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        target_schema,
      };

      alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
        .await
        .unwrap();

      conn
        .query(&format!("SELECT {pk_col}, new FROM foo"), ())
        .await?;
    }

    {
      // Rename table and remove "new" column.
      let mut target_schema = create_table_request.schema.clone();

      target_schema.name = "bar".to_string();

      info!("{}", target_schema.create_table_statement());

      let alter_table_request = AlterTableRequest {
        source_schema: create_table_request.schema.clone(),
        target_schema,
      };

      alter_table_handler(State(state.clone()), Json(alter_table_request.clone()))
        .await
        .unwrap();

      assert!(conn.query("SELECT * FROM foo", ()).await.is_err());
      conn.query(&format!("SELECT {pk_col} FROM bar"), ()).await?;
    }

    return Ok(());
  }
}
