use askama::Template;
use trailbase_schema::db::QualifiedNameEscaped;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_sqlite::{ConnectionType, SyncConnectionTrait};

use crate::auth::User;
use crate::config::proto;
use crate::records::Permission;
use crate::records::error::RecordError;
use crate::records::params::{InsertParams, InsertParamsRef, NamedParam, Params};
use crate::records::queries::WriteQueryResult;
use crate::records::record_api::RecordApi;
use crate::util::row_id_column;

#[derive(Debug)]
pub struct InsertOrReplaceQuery {
  query: String,
  params: InsertParams,
}

impl InsertOrReplaceQuery {
  pub fn new(
    connection_type: ConnectionType,
    table_name: &QualifiedNameEscaped,
    column_metadata: &[ColumnMetadata],
    pk_column_name: &str,
    conflict_resolution: proto::ConflictResolutionStrategy,
    params: InsertParams,
  ) -> Result<Self, RecordError> {
    let row_id_column = row_id_column(connection_type);
    let query = CreateOrReplaceRecordQueryTemplate {
      table_name,
      params: &params.params,
      pk_column_name,
      column_metadata: match conflict_resolution {
        proto::ConflictResolutionStrategy::Replace => column_metadata,
        proto::ConflictResolutionStrategy::Ignore => &[],
        proto::ConflictResolutionStrategy::Abort | proto::ConflictResolutionStrategy::Undefined => {
          &[]
        }
      },
      ignore_conflict: matches!(
        conflict_resolution,
        proto::ConflictResolutionStrategy::Ignore
      ),
      returning: &[row_id_column, pk_column_name],
    }
    .render()
    .map_err(|err| RecordError::Internal(err.into()))?;

    return Ok(Self { query, params });
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return if let Some(mut row) = conn.query_row(&self.query, InsertParamsRef(&self.params))? {
      Ok(WriteQueryResult {
        rowid: row.get(0)?,
        pk_value: Some(row.consume_value(1)?),
      })
    } else {
      Err(trailbase_sqlite::Error::QueryReturnedNoRows)
    };
  }
}

pub struct InsertOrReplaceRecordApiQuery {
  api: RecordApi,
  query: InsertOrReplaceQuery,
}

impl InsertOrReplaceRecordApiQuery {
  pub fn new(api: RecordApi, params: InsertParams) -> Result<Self, RecordError> {
    let conflict_resolution = api
      .insert_conflict_resolution_strategy()
      .unwrap_or(proto::ConflictResolutionStrategy::Undefined);

    return Ok(Self {
      query: InsertOrReplaceQuery::new(
        api.connection_type(),
        api.table_name(),
        api.columns(),
        &api.record_pk_column().column.name,
        conflict_resolution,
        params,
      )?,
      api,
    });
  }

  pub fn check_table_level_access(&self, user: Option<&User>) -> Result<(), RecordError> {
    return self.api.check_table_level_access(Permission::Create, user);
  }

  pub fn check_record_level_access(
    &self,
    conn: &mut impl SyncConnectionTrait,
    user: Option<&User>,
  ) -> Result<(), RecordError> {
    return self.api.check_record_level_access_only(
      conn,
      Permission::Create,
      None,
      Some(Params::Insert(&self.query.params)),
      user,
    );
  }

  pub fn apply(
    &self,
    conn: &mut impl SyncConnectionTrait,
  ) -> Result<WriteQueryResult, trailbase_sqlite::Error> {
    return self.query.apply(conn);
  }
}

#[derive(Template)]
#[template(escape = "none", path = "create_or_replace_record_query.sql")]
struct CreateOrReplaceRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  params: &'a [NamedParam],
  pk_column_name: &'a str,
  column_metadata: &'a [ColumnMetadata],
  ignore_conflict: bool,
  returning: &'a [&'a str],
}

#[cfg(test)]
mod tests {
  use super::*;

  use serde_json::json;
  use trailbase_schema::db::parse_sql::{Bump, parse_into_statement};
  use trailbase_schema::db::sqlite::Table;
  use trailbase_schema::db::{QualifiedName, QualifiedNameEscaped};
  use trailbase_sqlite::Value;

  use crate::config::proto;
  use crate::records::test_utils::json_row_from_value;
  use crate::schema_metadata::TableMetadata;

  fn sanitize_template(template: &str) {
    let allocator = Bump::new();
    let r = parse_into_statement(&allocator, template);
    assert!(r.is_ok(), "{template}: {:?}", r.err());
    assert!(!template.contains("\n\n"), "{template}");
    assert!(!template.contains("   "), "{template}");
  }

  #[test]
  fn test_create_record_template() {
    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        params: &[
          NamedParam {
            column_index: 0,
            column_name: "index".into(),
            placeholder: ":index".into(),
            value: Value::Null,
          },
          NamedParam {
            column_index: 1,
            column_name: "trigger".into(),
            placeholder: ":trigger".into(),
            value: Value::Null,
          },
        ],
        pk_column_name: "index",
        column_metadata: &[],
        ignore_conflict: false,
        returning: &["index"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName {
          name: "table".to_string(),
          database_schema: Some("db".to_string()),
        }
        .into(),
        params: &[],
        pk_column_name: "id",
        column_metadata: &[],
        ignore_conflict: false,
        returning: &["*"],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }

    {
      let query = CreateOrReplaceRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        params: &[NamedParam {
          column_index: 0,
          column_name: "index".into(),
          placeholder: ":index".into(),
          value: Value::Null,
        }],
        pk_column_name: "index",
        column_metadata: &[],
        ignore_conflict: true,
        returning: &[],
      }
      .render()
      .unwrap();

      sanitize_template(&query);
    }
  }

  #[test]
  fn test_empty_insert_params() {
    let table_name = "test_table".to_string();

    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(
      &allocator,
      &format!(
        "CREATE TABLE {table_name} (
          id      INTEGER PRIMARY KEY,
          data    TEXT
        ) STRICT;"
      ),
    )
    .unwrap()
    .unwrap())
      .try_into()
      .unwrap();

    let registry =
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap();
    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();
    let qualified_name = QualifiedNameEscaped::new(&QualifiedName {
      name: table_name.clone(),
      database_schema: None,
    });

    struct Input {
      input: serde_json::Value,
      expected_params: usize,
    }

    for i in [
      Input {
        input: json!({}),
        expected_params: 0,
      },
      Input {
        input: json!({"unknown_column": 5}),
        expected_params: 0,
      },
      Input {
        input: json!({"data": "test"}),
        expected_params: 1,
      },
      Input {
        input: json!({"id": 5, "data": "test"}),
        expected_params: 2,
      },
    ] {
      let (params, _files) = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(i.input).unwrap(),
        None,
      )
      .unwrap();

      assert_eq!(i.expected_params, params.params.len());

      let query = InsertOrReplaceQuery::new(
        ConnectionType::Sqlite,
        &qualified_name,
        &metadata.column_metadata,
        "id",
        proto::ConflictResolutionStrategy::Replace,
        params,
      )
      .unwrap();

      // Make sure it's a valid query
      assert!(
        parse_into_statement(&allocator, &query.query).is_ok(),
        "{query:?}"
      )
    }
  }
}
