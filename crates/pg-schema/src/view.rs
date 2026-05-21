use itertools::Itertools;
use serde::Deserialize;
use trailbase_schema::sqlite::{ColumnMapping, QualifiedName, View, ViewColumn};

use crate::error::Error;
use crate::table::{build_column_schema, get_columns};

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

  let columns = get_columns(conn, &table_name)?;

  let column_mapping = if let Ok(columns) = columns
    .into_iter()
    .map(build_column_schema)
    .collect::<Result<Vec<_>, _>>()
  {
    // FIXME: Implement proper ColumnMapping extraction.
    Some(ColumnMapping {
      columns: columns
        .into_iter()
        .map(|c| ViewColumn {
          column: c,
          parent_name: None,
          aggregation: None,
        })
        .collect(),
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
  use super::*;
  use crate::util::test_connection;

  #[tokio::test]
  async fn postgres_view_schema_test() {
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
}
