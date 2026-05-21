#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("Db: {0}")]
  Db(#[from] trailbase_sqlite::Error),
  #[error("FromSql: {0}")]
  FromSql(#[from] trailbase_sqlite::from_sql::FromSqlError),
  #[error("NotFound: {0}")]
  NotFound(String),
}
