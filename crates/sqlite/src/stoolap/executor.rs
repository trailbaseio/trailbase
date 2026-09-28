use crate::error::Error;

#[derive(Clone, Default)]
pub struct Options {}

#[derive(Clone)]
pub(crate) struct Executor {
  pub db: stoolap::Database,
}

#[allow(unused)]
impl Executor {
  pub fn new(db: stoolap::Database, opt: Options) -> Result<Self, Error> {
    return Ok(Self { db });
  }

  pub fn threads(&self) -> usize {
    return 1;
  }

  #[inline]
  pub(crate) async fn map(
    &self,
    f: impl Fn(&stoolap::Database) -> Result<(), Error> + Sync + Send + 'static,
  ) -> Result<(), Error> {
    return f(&self.db);
  }

  #[inline]
  pub async fn call<F, R, E>(&self, function: F) -> Result<R, Error>
  where
    F: FnOnce(&stoolap::Database) -> Result<R, E> + Send + 'static,
    R: Send + 'static,
    E: Send + 'static,
    Error: From<E>,
  {
    return Ok(function(&self.db)?);
  }

  pub(crate) fn close_impl(&self) -> Result<(), Error> {
    return Ok(self.db.close()?);
  }
}

#[cfg(test)]
mod tests {
  #[tokio::test]
  async fn stolap_poc_test() {}
}
