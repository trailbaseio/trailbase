use crate::error::Error;
use crate::from_sql::FromSql;
use crate::params::Params;
use crate::sqlite::util::get_value;

pub struct Transaction<'a> {
  tx: rusqlite::Transaction<'a>,
}

impl<'a> Transaction<'a> {
  pub fn new(tx: rusqlite::Transaction<'a>) -> Self {
    return Self { tx };
  }

  pub fn commit(self) -> Result<(), Error> {
    self.tx.commit()?;
    return Ok(());
  }

  pub fn rollback(self) -> Result<(), Error> {
    self.tx.rollback()?;
    return Ok(());
  }

  pub fn execute(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
  ) -> Result<usize, Error> {
    let mut stmt = self.tx.prepare_cached(sql.as_ref())?;
    params.bind(&mut stmt)?;
    return Ok(stmt.raw_execute()?);
  }

  pub async fn query_row_get<T>(
    &self,
    sql: impl AsRef<str> + Send + 'static,
    params: impl Params + Send + 'static,
    index: usize,
  ) -> Result<Option<T>, Error>
  where
    T: FromSql + Send + 'static,
  {
    let mut stmt = self.tx.prepare_cached(sql.as_ref())?;
    params.bind(&mut stmt)?;

    if let Some(row) = stmt.raw_query().next()? {
      return get_value(row, index);
    }
    return Ok(None);
  }
}
