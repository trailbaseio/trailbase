#[cfg(test)]
pub async fn test_connection() -> (
  oliphaunt_wasix::OliphauntServer,
  trailbase_sqlite::Connection,
) {
  let temp_dir = tempfile::TempDir::new().unwrap();

  let db = oliphaunt_wasix::OliphauntServer::builder()
    .listen(oliphaunt_wasix::ServerListen::unix(temp_dir.path()))
    .start()
    .unwrap();

  // The tests depend on the "template1" schema.
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
