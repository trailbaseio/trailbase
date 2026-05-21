use itertools::Itertools;
use serde::Deserialize;
use trailbase_schema::sqlite::{ColumnMapping, QualifiedName, View, ViewColumn};

use crate::error::Error;
use crate::table::{ColumnInformationSchema, build_column_schema, get_columns};

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ViewInformationSchema {
  pub table_catalog: String,
  pub table_schema: String,
  pub table_name: String,
  pub view_definition: String,
}

const QUERY_VIEWS: &str = "
SELECT
    table_catalog,
    table_schema,
    table_name,
    view_definition
FROM
    information_schema.views
WHERE
    table_schema NOT IN ('information_schema', 'pg_catalog')
ORDER BY
    table_schema,
    table_name;
";

fn get_views(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<ViewInformationSchema>, Error> {
  return conn
    .query_rows(QUERY_VIEWS, ())?
    .into_iter()
    .map(|row| {
      return Ok(ViewInformationSchema {
        table_catalog: row.get(0)?,
        table_schema: row.get(1)?,
        table_name: row.get(2)?,
        view_definition: row.get::<String>(3)?.split_whitespace().join(" "),
      });
    })
    .collect::<Result<_, Error>>();
}

fn build_view_column_schema(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
  c: ColumnInformationSchema,
) -> Result<ViewColumn, Error> {
  return match (&c.source_table, &c.source_column) {
    (Some(source_table), Some(source_column)) => {
      let source_columns = get_columns(conn, source_table)?;
      let Some(source_column) = source_columns
        .into_iter()
        .find(|c| c.column_name == *source_column)
      else {
        return Err(Error::NotFound(format!(
          "Source column: {source_table}.{source_column}"
        )));
      };

      Ok(ViewColumn {
        column: {
          let mut column = build_column_schema(source_column)?;
          column.name = c.column_name;
          column
        },
        parent_name: Some(source_table.clone()),
        aggregation: None,
      })
    }
    _ => Ok(ViewColumn {
      column: build_column_schema(c)?,
      parent_name: None,
      aggregation: None,
    }),
  };
}

pub fn build_view_schema(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
  view: ViewInformationSchema,
) -> Result<View, Error> {
  let ViewInformationSchema {
    table_catalog: _,
    table_schema,
    table_name,
    view_definition,
  } = view;

  let column_mapping = if let Ok(columns) = get_columns(conn, &table_name)?
    .into_iter()
    .map(|c| build_view_column_schema(conn, c))
    .collect::<Result<Vec<_>, _>>()
  {
    // FIXME: Implement better ColumnMapping extraction, including view-level constraints and column
    // properties.
    Some(ColumnMapping {
      columns,
      group_by: None,
      joins: vec![],
    })
  } else {
    None
  };

  return Ok(View {
    name: QualifiedName {
      name: table_name,
      database_schema: Some(table_schema),
    },
    // TODO: extract column mapping. We can either query PG some more or parse the view
    // definition with something like: https://crates.io/crates/sqlparser.
    column_mapping,
    query: view_definition,
    temporary: false,
    if_not_exists: false,
  });
}

pub fn build_all_view_schemas(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<View>, Error> {
  let views = get_views(conn)?;

  return views
    .into_iter()
    .map(|view| build_view_schema(conn, view))
    .collect::<Result<Vec<View>, Error>>();
}

#[cfg(test)]
mod tests {
  use trailbase_schema::sqlite::{Column, ColumnOption};

  use super::*;
  use crate::util::test_connection;

  #[tokio::test]
  async fn postgres_view_schema_simple_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch("CREATE VIEW view_name AS SELECT 5 AS i, 'text' AS t;")
      .await
      .unwrap();

    let views = conn
      .call_writer(|mut conn| {
        return build_all_view_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(1, views.len());
    assert_eq!("SELECT 5 AS i, 'text'::text AS t;", views[0].query);
  }

  #[tokio::test]
  async fn postgres_view_schema_derived_column_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE TABLE tt (id SERIAL PRIMARY KEY, value INTEGER NOT NULL);

        CREATE VIEW vv AS SELECT * FROM
          (SELECT 5),
          (SELECT value FROM tt),
          (SELECT CONCAT('hello: ', value) FROM tt)
        ;
        ",
      )
      .await
      .unwrap();

    let views = conn
      .call_writer(|mut conn| {
        return build_all_view_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(1, views.len());

    let column_mapping = views[0].column_mapping.as_ref().unwrap();
    assert_eq!(3, column_mapping.columns.len());

    assert_eq!(
      ViewColumn {
        column: Column {
          name: "?column?".to_string(),
          type_name: "integer".to_string(),
          data_type: trailbase_schema::sqlite::ColumnDataType::Integer,
          affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Integer,
          options: vec![],
        },
        parent_name: None,
        aggregation: None,
      },
      column_mapping.columns[0],
    );

    assert_eq!(
      ViewColumn {
        column: Column {
          name: "value".to_string(),
          type_name: "integer".to_string(),
          data_type: trailbase_schema::sqlite::ColumnDataType::Integer,
          affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Integer,
          options: vec![ColumnOption::NotNull],
        },
        parent_name: Some("tt".to_string()),
        aggregation: None,
      },
      column_mapping.columns[1],
    );

    assert_eq!(
      ViewColumn {
        column: Column {
          name: "concat".to_string(),
          type_name: "text".to_string(),
          data_type: trailbase_schema::sqlite::ColumnDataType::Text,
          affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Text,
          options: vec![],
        },
        parent_name: None,
        aggregation: None,
      },
      column_mapping.columns[2],
    );
  }
}
