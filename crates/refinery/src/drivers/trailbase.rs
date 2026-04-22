use crate::Migration;
use crate::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use async_trait::async_trait;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use trailbase_sqlite::{Connection, Error, SyncConnectionTrait};

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
      .transaction(move |tx| -> Result<_, Error> {
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

impl AsyncMigrate for Connection {}
