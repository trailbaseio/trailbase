#[cfg(test)]
pub async fn test_connection() -> (pglite_oxide::PgliteServer, trailbase_sqlite::Connection) {
  let temp_dir = tempfile::TempDir::new().unwrap();

  // NOTE: `db.connection_uri()` returns rubbish for UDS.
  let sock = temp_dir.path().join(".s.PGSQL.5432");

  let db = pglite_oxide::PgliteServer::builder()
    .fresh_temporary()
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
