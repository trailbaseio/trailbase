use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct ColumnSchema {
  table_catalog: String,
}

#[cfg(test)]
mod tests {
  use trailbase_sqlite::Connection;

  use super::*;

  pub async fn test_connection() -> (pglite_oxide::PgliteServer, Connection) {
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
  async fn postgres_schema_test() {
    let (_db, conn) = test_connection().await;

    let schemas: Vec<ColumnSchema> = conn
      .read_query_values("SELECT * FROM information_schema.columns", ())
      .await
      .unwrap();

    assert!(schemas.len() > 0);
  }
}
