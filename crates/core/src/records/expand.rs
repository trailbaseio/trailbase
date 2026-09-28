use itertools::Itertools;
use trailbase_schema::db::QualifiedName;
use trailbase_schema::db::sqlite::ColumnOption;

use crate::records::RecordError;
use crate::records::record_api::RecordApi;
use crate::schema_metadata::{ConnectionMetadata, TableMetadata};

pub(crate) struct ExpandedTable<'a> {
  pub metadata: &'a TableMetadata,
  pub local_column_name: String,
  pub num_columns: usize,

  pub foreign_table_name: String,
  pub foreign_column_name: String,
}

pub(crate) fn expand_tables<'s, T: AsRef<str>>(
  record_api: &'s RecordApi,
  connection_metadata: &'s ConnectionMetadata,
  expand: &[T],
) -> Result<Vec<ExpandedTable<'s>>, RecordError> {
  let mut expanded_tables = Vec::<ExpandedTable>::with_capacity(expand.len());

  for col_name in expand {
    let col_name = col_name.as_ref();
    if col_name.is_empty() {
      continue;
    }

    let Some(meta) = record_api
      .column_index_by_name(col_name)
      .map(|idx| &record_api.columns()[idx])
    else {
      return Err(RecordError::Internal("Missing column".into()));
    };

    // FIXME: This only expand FKs expressed as column constraints missing table constraints.
    let Some(ColumnOption::ForeignKey {
      foreign_table: foreign_table_name,
      referred_columns: _,
      on_delete: _,
      on_update: _,
    }) = meta
      .column
      .options
      .iter()
      .find_or_first(|o| matches!(o, ColumnOption::ForeignKey { .. }))
    else {
      return Err(RecordError::Internal("not a foreign key".into()));
    };

    let fq_foreign_table_name = QualifiedName {
      name: foreign_table_name.clone(),
      database_schema: record_api.qualified_name().database_schema.clone(),
    };

    let Some(foreign_table) = connection_metadata.get_table(&fq_foreign_table_name) else {
      return Err(RecordError::ApiRequiresTable);
    };

    let Some(foreign_pk_column_idx) = foreign_table.record_pk_column else {
      return Err(RecordError::Internal("invalid PK".into()));
    };

    let foreign_pk_column = &foreign_table.schema.columns[foreign_pk_column_idx].name;

    // TODO: Check that `referred_columns` and foreign_pk_column are the same. It's already
    // validated as part of config validation.

    let num_columns = foreign_table.schema.columns.len();
    let foreign_table_name = foreign_table_name.to_string();
    let foreign_column_name = foreign_pk_column.to_string();

    expanded_tables.push(ExpandedTable {
      metadata: foreign_table,
      local_column_name: col_name.to_string(),
      num_columns,
      foreign_table_name,
      foreign_column_name,
    });
  }

  return Ok(expanded_tables);
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::app_state::*;
  use crate::records::test_utils::*;
  use crate::schema_metadata::{TableMetadata, lookup_and_parse_table_schema};

  #[tokio::test]
  async fn test_read_rows() {
    let pattern = serde_json::from_str(
      r#"{
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "name": {
              "type": "string"
            },
            "obj": {
              "type": "object"
            }
          },
          "required": ["name", "obj"]
        }"#,
    )
    .unwrap();

    let state = test_state(Some(TestStateOptions {
      json_schema_registry: Some(
        trailbase_schema::json_schema::registry::build_json_schema_registry(vec![(
          "foo".to_string(),
          pattern,
        )])
        .unwrap(),
      ),
      ..Default::default()
    }))
    .await
    .unwrap();
    let conn = state.conn();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE test_table (
            col0   {json} CHECK(jsonschema('foo', col0))
          );
        "#,
        json = json_column(conn)
      ))
      .await
      .unwrap();

    let table = lookup_and_parse_table_schema(conn, "test_table", Some("main"))
      .await
      .unwrap();

    let metadata = TableMetadata::new(
      &state.json_schema_registry().read(),
      table.clone(),
      &[table],
    )
    .unwrap();

    let insert = |json: serde_json::Value| async move {
      let query = format!(
        "INSERT INTO test_table (col0) VALUES ('{}')",
        serde_json::to_string(&json).unwrap()
      );
      conn.execute(query, ()).await
    };

    let object = json!({"name": "foo", "obj": json!({
      "a": "b",
      "c": 42,
    })});

    #[cfg(feature = "pg-test")]
    {
      let err = insert(object.clone()).await.err().unwrap();
      match err {
        trailbase_sqlite::Error::Postgres(err) => {
          assert!(format!("{err:?}").contains("jsonschema"));
        }
        _ => {
          panic!("unexpected error: {err}");
        }
      }
    }

    // PG doesn't (yet) support custom schemas as defined for column `col0` above.
    #[cfg(not(feature = "pg-test"))]
    {
      insert(object.clone()).await.unwrap();

      let rows = conn
        .read_query_rows("SELECT * FROM test_table", ())
        .await
        .unwrap();

      let records: Vec<_> = rows
        .into_iter()
        .map(|row| {
          let obj = trailbase_schema::json::record_to_json_expand(
            &metadata.column_metadata,
            &[],
            &row,
            None,
          )
          .unwrap();

          return to_object(&obj);
        })
        .collect();

      assert_eq!(records.len(), 1);
      assert_eq!(
        records.first().unwrap().get("col0").unwrap().clone(),
        object
      );
    }
  }
}
