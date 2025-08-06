use crate::Migration;
use crate::traits::sync::{Migrate, Query, Transaction};
use rusqlite::{Connection as RqlConnection, Error as RqlError};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

fn query_applied_migrations(
  transaction: &RqlConnection,
  query: &str,
) -> Result<Vec<Migration>, RqlError> {
  let mut stmt = transaction.prepare(query)?;
  let mut rows = stmt.query([])?;
  let mut applied = Vec::new();
  while let Some(row) = rows.next()? {
    let version = row.get(0)?;
    let applied_on: String = row.get(2)?;
    // Safe to call unwrap, as we stored it in RFC3339 format on the database
    let applied_on = OffsetDateTime::parse(&applied_on, &Rfc3339).unwrap();

    let checksum: String = row.get(3)?;
    applied.push(Migration::applied(
      version,
      row.get(1)?,
      applied_on,
      checksum
        .parse::<u64>()
        .expect("checksum must be a valid u64"),
    ));
  }
  Ok(applied)
}

impl Transaction for RqlConnection {
  type Error = RqlError;
  fn execute<'a, T: Iterator<Item = &'a str>>(&mut self, queries: T) -> Result<usize, Self::Error> {
    let transaction = self.transaction()?;
    let mut count = 0;
    for query in queries {
      transaction.execute_batch(query)?;
      count += 1;
    }
    transaction.commit()?;
    Ok(count)
  }
}

impl Query<Vec<Migration>> for RqlConnection {
  fn query(&mut self, query: &str) -> Result<Vec<Migration>, Self::Error> {
    let transaction = self.transaction()?;
    let applied = query_applied_migrations(&transaction, query)?;
    transaction.commit()?;
    Ok(applied)
  }
}

impl Migrate for RqlConnection {}
