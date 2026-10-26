#[cfg(test)]
pub async fn test_connection() -> (
  testcontainers_modules::testcontainers::ContainerAsync<
    testcontainers_modules::postgres::Postgres,
  >,
  trailbase_sqlite::Connection,
) {
  use testcontainers_modules::postgres::Postgres;
  use testcontainers_modules::testcontainers::{ImageExt, runners::AsyncRunner};

  let db = Postgres::default()
    .with_init_sql(
      b"\
        CREATE EXTENSION IF NOT EXISTS citext; \
        CREATE EXTENSION IF NOT EXISTS pgcrypto; \
      "
      .to_vec(),
    )
    .with_tag("18-alpine")
    .start()
    .await
    .unwrap();

  let host = db.get_host().await.unwrap();
  let port = db.get_host_port_ipv4(5432).await.unwrap();
  let pg_uri = format!(
    "{scheme}://postgres:postgres@{host}:{port}/postgres",
    scheme = "postgresql"
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
