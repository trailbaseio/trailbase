use crate::Migration;
use crate::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use async_trait::async_trait;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use trailbase_sqlite::{Connection, ConnectionType, Error};

#[async_trait]
impl AsyncTransaction for Connection {
  type Error = Error;

  async fn execute<'a, T: Iterator<Item = &'a str> + Send>(
    &mut self,
    queries: T,
  ) -> Result<usize, Self::Error> {
    let is_sqlite = self.connection_type() == ConnectionType::Sqlite;
    let initial_fk: bool = is_sqlite
      && self
        .read_query_row_get("PRAGMA foreign_keys;", (), 0)
        .await?
        .unwrap_or(true);
    if initial_fk {
      // Turn off foreign key constraints temporarily (re-enabled as part of the transaction)
      // to allow for a wider range of migrations.
      //
      // Ideally, we'd use `defer_foreign_key=ON` as part of the migration within the transaction,
      // but it somehow doesn't seem to work or be less lenient than `foreign_keys=OFF`, which has
      // to be applied to the connection rather than the transaction.
      self.execute_batch("PRAGMA foreign_keys = OFF;").await?;
    }

    let result = self
      .transaction({
        let queries: Vec<String> = queries.map(|q| q.to_string()).collect();
        move |tx| apply_migrations(tx, &queries, initial_fk)
      })
      .await;

    if initial_fk {
      self.execute_batch("PRAGMA foreign_keys = ON;").await?;
    }

    return result;
  }
}

#[async_trait]
impl AsyncQuery<Vec<Migration>> for Connection {
  async fn query(
    &mut self,
    query: &str,
  ) -> Result<Vec<Migration>, <Self as AsyncTransaction>::Error> {
    let query = query.to_string();
    return self
      .call_reader(move |mut conn| query_applied_migrations(&mut conn, &query))
      .await;
  }
}

impl AsyncMigrate for Connection {
  fn assert_migrations_table_query(&self, migration_table_name: &str) -> String {
    const ASSERT_SQLITE_MIGRATION_TABLE_QUERY: &str = "\
        CREATE TABLE IF NOT EXISTS %MIGRATION_TABLE_NAME%(\
          version INT PRIMARY KEY,
          name TEXT,
          applied_on TEXT,
          checksum TEXT
        ) STRICT;";

    const ASSERT_PG_MIGRATION_TABLE_QUERY: &str = "\
        CREATE TABLE IF NOT EXISTS %MIGRATION_TABLE_NAME%(\
          version INT PRIMARY KEY,
          name TEXT,
          applied_on TEXT,
          checksum TEXT
        );";

    return match self.connection_type() {
      ConnectionType::Sqlite => {
        ASSERT_SQLITE_MIGRATION_TABLE_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name)
      }
      ConnectionType::Pg => {
        ASSERT_PG_MIGRATION_TABLE_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name)
      }
    };
  }
}

pub(super) fn query_applied_migrations<C: SyncConnection>(
  conn: &mut C,
  query: &str,
) -> Result<Vec<Migration>, Error> {
  let rows = conn.query_rows(query.to_string(), ())?;

  return rows
    .iter()
    .map(|row| {
      let version = row.get(0)?;
      let applied_on: String = row.get(2)?;
      let applied_on =
        OffsetDateTime::parse(&applied_on, &Rfc3339).map_err(|err| Error::Other(err.into()))?;
      let checksum: String = row.get(3)?;

      return Ok(Migration::applied(
        version,
        row.get(1)?,
        applied_on,
        checksum
          .parse::<u64>()
          .map_err(|err| Error::Other(err.into()))?,
      ));
    })
    .collect::<Result<Vec<_>, Error>>();
}

pub(super) fn apply_migrations<'a>(
  mut tx: impl SyncTransaction,
  queries: &[String],
  foreign_keys: bool,
) -> Result<usize, Error> {
  let mut count = 0;
  for query in queries {
    tx.execute_batch(query)?;
    count += 1;
  }

  // Check for potential foreign key violations before committing.
  // Setting the "foreign_keys" PRAGMA in a transaction is a no-op:
  //   https://www.sqlite.org/pragma.html#pragma_foreign_keys
  if foreign_keys {
    let violations: Vec<String> = tx
      .query_rows("PRAGMA foreign_key_check;", ())?
      .iter()
      .map(|r| r.get::<String>(0))
      .collect::<Result<_, _>>()?;

    if !violations.is_empty() {
      return Err(Error::Other(
        format!("FK violations: {violations:?}").into(),
      ));
    }
  }

  tx.commit()?;

  return Ok(count);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn test<T>(_fut: impl futures_util::Future<Output = T> + Send) {}

  #[tokio::test]
  async fn trailbase_refinery_test() {
    let mut conn = Connection::open_in_memory().unwrap();

    let runner = crate::Runner::new(&vec![]);
    runner.run_async(&mut conn).await.unwrap();

    test(Box::pin(async move {
      let runner = crate::Runner::new(&vec![]);
      return runner.run_async(&mut conn).await;
    }));
  }
}
