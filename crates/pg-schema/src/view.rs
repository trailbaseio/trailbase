use serde::Deserialize;
use trailbase_schema::sqlite::View;

use crate::error::Error;

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
        view_definition: row.get::<String>(3)?.trim().to_string(),
      });
    })
    .collect::<Result<_, Error>>();
}

pub fn build_view_schema(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
  view: ViewInformationSchema,
) -> Result<View, Error> {
  panic!("not implemented");
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
  use trailbase_sqlite::Connection;

  use super::*;

  async fn test_connection() -> (pglite_oxide::PgliteServer, Connection) {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // NOTE: `db.connection_uri()` returns rubbish for UDS.
    let sock = temp_dir.path().join(".s.PGSQL.5432");

    let db = pglite_oxide::PgliteServer::builder()
      .fresh_temporary()
      // .temporary()
      .unix(&sock)
      .start()
      .unwrap();

    let pg_uri = format!(
      "postgresql://postgres@/template1?host={}",
      temp_dir.path().to_string_lossy()
    );

    return (
      db,
      trailbase_sqlite::Connection::pg_with_opts(trailbase_sqlite::generic::PgOptions {
        connection: trailbase_sqlite::generic::PgConnection::Uri(pg_uri),
        num_threads: Some(1),
      })
      .unwrap(),
    );
  }

  #[tokio::test]
  async fn postgres_view_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch("CREATE VIEW view_name AS SELECT 5 AS i, 'text' AS t;")
      .await
      .unwrap();

    let views = conn
      .call_writer(|mut conn| {
        return get_views(&mut conn).map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(1, views.len());
    assert_eq!(
      "SELECT 5 AS i,\n    'text'::text AS t;",
      views[0].view_definition
    );
  }
}
