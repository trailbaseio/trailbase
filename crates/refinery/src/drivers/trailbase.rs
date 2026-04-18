use crate::Migration;
use crate::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use async_trait::async_trait;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use trailbase_sqlite::{Connection, Error, Transaction};

fn query_applied_migrations(
  transaction: Transaction<'_>,
  query: &str,
) -> Result<Vec<Migration>, Error> {
  let rows = transaction.query_rows(query, ())?;

  let mut applied = Vec::new();
  for row in rows {
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

  return Ok(applied);
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
      .transaction(move |tx| -> Result<_, Error> {
        let mut count = 0;
        for query in queries {
          tx.execute(query, ())?;
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
    let query = query.to_string();
    let applied = self
      .transaction(move |tx| -> Result<_, Error> {
        let applied = query_applied_migrations(tx, &query)?;
        // tx.rollback()?;
        return Ok(applied);
      })
      .await?;

    return Ok(applied);
  }
}

impl AsyncMigrate for Connection {}
