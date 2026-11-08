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
    dry_run: bool,
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

    let count = self
      .transaction({
        let queries: Vec<String> = queries.map(|q| q.to_string()).collect();
        move |mut tx| -> Result<usize, Error> {
          let count = apply_migrations(&mut tx, queries.iter().map(|s| s.as_str()), initial_fk)?;

          if !dry_run {
            tx.commit()?;
          } else {
            tx.rollback()?;
          }

          return Ok(count);
        }
      })
      .await;

    if initial_fk {
      self.execute_batch("PRAGMA foreign_keys = ON;").await?;
    }

    return count;
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
  let rows = conn.query_rows(query, ())?;

  return rows
    .iter()
    .map(|row| {
      let version = row.get(0)?;
      let name: String = row.get(1)?;
      let applied_on: String = row.get(2)?;
      let checksum: String = row.get(3)?;

      return Ok(Migration::applied(
        version,
        name,
        OffsetDateTime::parse(&applied_on, &Rfc3339).map_err(|err| Error::Other(err.into()))?,
        checksum
          .parse::<u64>()
          .map_err(|err| Error::Other(err.into()))?,
      ));
    })
    .collect::<Result<Vec<_>, Error>>();
}

pub(super) fn apply_migrations<'a>(
  tx: &mut impl SyncTransaction,
  queries: impl Iterator<Item = &'a str>,
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

  return Ok(count);
}

#[cfg(test)]
mod tests {
  use std::fs::File;
  use std::io::Write;

  use super::*;
  use crate::load_sql_migrations;

  fn write_migration(path: impl AsRef<std::path::Path>, migration: &str) {
    let mut v0 = File::create(path).unwrap();
    v0.write_all(migration.as_bytes()).unwrap();
  }

  #[tokio::test]
  async fn trailbase_refinery_test() {
    const V0: &str = "
        CREATE TABLE test (
            id     INTEGER PRIMARY KEY,
            data   INTEGER
        );
        INSERT INTO test (data) VALUES (42);
    ";

    let temp_dir = temp_dir::TempDir::new().unwrap();
    let mut conn = Connection::open_in_memory().unwrap();

    write_migration(temp_dir.child("V0__initial.sql"), V0);
    let migrations = load_sql_migrations(temp_dir.path()).unwrap();
    assert_eq!(1, migrations.len());

    {
      let runner = crate::Runner::new(&migrations).set_dry_run(true);
      runner.run_async(&mut conn).await.unwrap();

      assert!(
        conn
          .read_query_row_get::<i64>("SELECT COUNT(*) FROM test", (), 0)
          .await
          .is_err()
      );
    }

    {
      let runner = crate::Runner::new(&migrations);
      runner.run_async(&mut conn).await.unwrap();

      assert_eq!(
        1,
        conn
          .read_query_row_get::<i64>("SELECT COUNT(*) FROM test", (), 0)
          .await
          .unwrap()
          .unwrap()
      )
    }
  }
}
