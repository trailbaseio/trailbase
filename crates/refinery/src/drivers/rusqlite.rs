use rusqlite::{Connection as RqlConnection, Error as RqlError};

use crate::Migration;
use crate::drivers::trailbase::{apply_migrations, query_applied_migrations};
use crate::traits::sync::{Migrate, Query, Transaction};

impl Transaction for RqlConnection {
  type Error = RqlError;

  fn execute<'a, T: Iterator<Item = &'a str>>(&mut self, queries: T) -> Result<usize, Self::Error> {
    let initial_fk: bool = self.query_one("PRAGMA foreign_keys;", (), |row| row.get(0))?;
    if initial_fk {
      // Turn off foreign key constraints temporarily (re-enabled as part of the transaction)
      // to allow for a wider range of migrations.
      //
      // Ideally, we'd use `defer_foreign_key=ON` as part of the migration within the transaction,
      // but it somehow doesn't seem to work or be less lenient than `foreign_keys=OFF`, which has
      // to be applied to the connection rather than the transaction.
      self.pragma_update(None, "foreign_keys", false)?;
    }

    let queries: Vec<String> = queries.map(|q| q.to_string()).collect();
    let tx = self.transaction()?;
    let result = apply_migrations(tx, &queries, initial_fk).map_err(|err| match err {
      trailbase_sqlite::Error::Rusqlite(err) => err,
      _ => RqlError::UserFunctionError(err.into()),
    });
    if initial_fk {
      self.pragma_update(None, "foreign_keys", true)?;
    }

    return result;
  }
}

impl Query<Vec<Migration>> for RqlConnection {
  fn query(&mut self, query: &str) -> Result<Vec<Migration>, Self::Error> {
    return Ok(
      query_applied_migrations(self, query).map_err(|err| match err {
        trailbase_sqlite::Error::Rusqlite(err) => err,
        _ => RqlError::UserFunctionError(err.into()),
      })?,
    );
  }
}

impl Migrate for RqlConnection {
  fn assert_migrations_table_query(&self, migration_table_name: &str) -> String {
    const ASSERT_MIGRATIONS_TABLE_QUERY: &str = "CREATE TABLE IF NOT EXISTS %MIGRATION_TABLE_NAME%(
             version INT PRIMARY KEY,
             name TEXT,
             applied_on TEXT,
             checksum TEXT
   ) STRICT;";

    return ASSERT_MIGRATIONS_TABLE_QUERY.replace("%MIGRATION_TABLE_NAME%", migration_table_name);
  }
}

#[cfg(test)]
mod tests {
  use crate::*;
  use std::fs::File;
  use std::io::Write;

  fn write_migration(path: impl AsRef<std::path::Path>, migration: &str) {
    let mut v0 = File::create(path).unwrap();
    v0.write_all(migration.as_bytes()).unwrap();
  }

  #[test]
  fn test_rusqlite_migrations_with_temporary_fk_violations() {
    const V0: &str = "
        CREATE TABLE _user (
            id     INTEGER PRIMARY KEY,
            email  TEXT NOT NULL
        ) STRICT;

        INSERT INTO _user (id, email) VALUES (42, 'admin@test.org');

        CREATE UNIQUE INDEX __user_email ON _user (email);

        CREATE TABLE test (
            id     INTEGER PRIMARY KEY,
            user   INTEGER REFERENCES _user(id)
        );

        INSERT INTO test (user) VALUES (42);
    ";
    const U4: &str = "
        CREATE TABLE temporary_user (
            id     INTEGER PRIMARY KEY,
            email  TEXT NOT NULL,
            new    TEXT
        ) STRICT;

        INSERT INTO temporary_user (id, email) SELECT id, email FROM _user;

        DROP TABLE _user;
        ALTER TABLE temporary_user RENAME TO _user;
    ";

    const INDEX_EXISTS: &str =
      "SELECT EXISTS(SELECT * FROM sqlite_schema WHERE type = 'index' AND name = '__user_email');";

    let temp_dir = temp_dir::TempDir::new().unwrap();
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();

    {
      write_migration(temp_dir.child("V0__initial.sql"), V0);

      let migrations = load_sql_migrations(temp_dir.path()).unwrap();
      assert_eq!(1, migrations.len());
      Runner::new(&migrations)
        .set_abort_divergent(false)
        .set_grouped(false)
        .run(&mut conn)
        .unwrap();

      assert_eq!(
        true,
        conn
          .query_one(INDEX_EXISTS, (), |row| row.get::<_, bool>(0))
          .unwrap()
      );
    }

    {
      write_migration(temp_dir.child("U4__changes.sql"), U4);

      let migrations = load_sql_migrations(temp_dir.path()).unwrap();
      assert_eq!(2, migrations.len());

      Runner::new(&migrations)
        .set_abort_divergent(false)
        .set_grouped(false)
        .run(&mut conn)
        .unwrap();

      conn.pragma_update(None, "foreign_keys", true).unwrap();
      let num_users: i64 = conn
        .query_one("SELECT COUNT(*) FROM _user;", (), |row| row.get(0))
        .unwrap();

      assert_eq!(1, num_users);

      assert_eq!(
        false,
        conn
          .query_one(INDEX_EXISTS, (), |row| row.get::<_, bool>(0))
          .unwrap()
      );

      assert_eq!(
        true,
        conn
          .query_one("PRAGMA foreign_keys", (), |row| row.get::<_, bool>(0))
          .unwrap()
      );
    }
  }
}
