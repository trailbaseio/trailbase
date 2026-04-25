use crate::error::Error;
use crate::params::Params;
use crate::rows::{Row, Rows};

pub trait SyncConnection {
  /// Queries the first row and returns it if present, otherwise `None`.
  fn query_row(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<Row>, Error>;

  /// Queries all rows. Materialization is eager, thus be careful with large results.
  /// We may want to introduce a lazy version in the future.
  fn query_rows(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Rows, Error>;

  /// Executes the query and returns number of affected rows.
  fn execute(&self, sql: impl AsRef<str>, params: impl Params) -> Result<usize, Error>;

  /// Executes a batch of statements.
  fn execute_batch(&self, sql: impl AsRef<str>) -> Result<(), Error>;
}

pub trait SyncTransaction: SyncConnection {
  /// Commit transaction.
  fn commit(self) -> Result<(), Error>;

  /// Rollback transaction.
  fn rollback(self) -> Result<(), Error>;

  /// Bind parameters and expand SQL.
  fn expand_sql(&self, sql: impl AsRef<str>, params: impl Params) -> Result<Option<String>, Error>;
}
