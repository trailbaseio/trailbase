use serde::Deserialize;
use trailbase_schema::sqlite::{QualifiedName, TableIndex};

use crate::error::Error;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PgIndex {
  pub schemaname: String,
  pub tablename: String,
  pub indexname: String,
  pub indexdef: String,
}

const QUERY_INDEXES: &str = "
SELECT
  schemaname,
  tablename,
  indexname,
  indexdef
FROM
  pg_indexes
WHERE
  schemaname NOT IN ('information_schema', 'pg_catalog')
ORDER BY
  schemaname,
  tablename,
  indexname;
";

fn get_indexes(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<PgIndex>, Error> {
  return conn
    .query_rows(QUERY_INDEXES, ())?
    .into_iter()
    .map(|row| {
      return Ok(PgIndex {
        schemaname: row.get(0)?,
        tablename: row.get(1)?,
        indexname: row.get(2)?,
        indexdef: row.get(3)?,
      });
    })
    .collect::<Result<_, Error>>();
}

pub fn build_index_schema(index: PgIndex) -> Result<(TableIndex, String), Error> {
  let PgIndex {
    schemaname,
    tablename,
    indexname,
    indexdef,
  } = index;

  return Ok((
    TableIndex {
      name: QualifiedName {
        name: indexname,
        database_schema: Some(schemaname),
      },
      table_name: tablename,
      columns: vec![],
      // The following are more SQLite AST related and functionally useful.
      unique: false,
      predicate: None,
      if_not_exists: false,
    },
    indexdef,
  ));
}

pub fn build_all_index_schemas(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<(TableIndex, String)>, Error> {
  return get_indexes(conn)?
    .into_iter()
    .map(build_index_schema)
    .collect();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::util::test_connection;

  #[tokio::test]
  async fn postgres_index_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
          CREATE TABLE films (
            id       INTEGER PRIMARY KEY,
            title    TEXT NOT NULL,
            t_min    INTEGER
          );

          CREATE UNIQUE INDEX __title_idx ON films (title);
        ",
      )
      .await
      .unwrap();

    let indexes = conn
      .call_writer(|mut conn| {
        return build_all_index_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(
      vec!["__title_idx", "films_pkey"],
      indexes
        .iter()
        .map(|idx| &idx.0.name.name)
        .collect::<Vec<_>>(),
      "{indexes:?}"
    );
  }
}
