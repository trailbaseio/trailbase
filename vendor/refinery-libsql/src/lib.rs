use async_trait::async_trait;
use libsql::{params, Connection, Error as LibsqlError, Transaction};
use refinery_core::traits::r#async::{AsyncMigrate, AsyncQuery, AsyncTransaction};
use refinery_core::Migration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

pub struct LibsqlConnection(Connection);

impl LibsqlConnection {
  pub fn from_connection(c: Connection) -> Self {
    Self(c)
  }
}

impl std::ops::Deref for LibsqlConnection {
  type Target = Connection;

  #[inline]
  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

async fn query_applied_migrations(
  transaction: &Transaction,
  query: &str,
) -> Result<Vec<Migration>, LibsqlError> {
  let mut rows = transaction.query(query, params![]).await?;
  let mut applied = Vec::new();
  loop {
    // for row in rows.into_iter()
    let Some(row) = rows.next().await? else {
      break;
    };

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

#[async_trait]
impl AsyncTransaction for LibsqlConnection {
  type Error = LibsqlError;

  async fn execute<'a, T: Iterator<Item = &'a str> + Send>(
    &mut self,
    queries: T,
  ) -> Result<usize, Self::Error> {
    let transaction = self.0.transaction().await?;
    let mut count = 0;
    for query in queries {
      transaction.execute_batch(query).await?;
      count += 1;
    }
    transaction.commit().await?;
    Ok(count as usize)
  }
}

#[async_trait]
impl AsyncQuery<Vec<Migration>> for LibsqlConnection {
  async fn query(
    &mut self,
    query: &str,
  ) -> Result<Vec<Migration>, <Self as AsyncTransaction>::Error> {
    let transaction = self.0.transaction().await?;
    let applied = query_applied_migrations(&transaction, query).await?;
    transaction.commit().await?;
    Ok(applied)
  }
}

impl AsyncMigrate for LibsqlConnection {}
