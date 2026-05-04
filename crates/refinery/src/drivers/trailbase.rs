use crate::Migration;
use crate::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use async_trait::async_trait;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use trailbase_sqlite::{Connection, ConnectionType, Error};

async fn query_applied_migrations(conn: &Connection, query: &str) -> Result<Vec<Migration>, Error> {
  let rows = conn.read_query_rows(query.to_string(), ()).await?;

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

#[async_trait]
impl AsyncTransaction for Connection {
  type Error = Error;

  async fn execute<'a, T: Iterator<Item = &'a str> + Send>(
    &mut self,
    queries: T,
  ) -> Result<usize, Self::Error> {
    let queries: Vec<String> = queries.map(|q| q.to_string()).collect();

    return self
      .transaction(move |mut tx| -> Result<_, Error> {
        let mut count = 0;
        for query in queries {
          tx.execute_batch(query)?;
          count += 1;
        }

        tx.commit()?;

        return Ok(count);
      })
      .await;
  }
}

#[async_trait]
impl AsyncQuery<Vec<Migration>> for Connection {
  async fn query(
    &mut self,
    query: &str,
  ) -> Result<Vec<Migration>, <Self as AsyncTransaction>::Error> {
    return query_applied_migrations(self, query).await;
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
