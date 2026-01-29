#[derive(thiserror::Error, Debug)]
#[allow(unused)]
pub enum BenchmarkError {
  #[error("Other error: {0}")]
  Other(Box<dyn std::error::Error + Sync + Send>),

  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("TrailBase error: {0}")]
  TrailBase(#[from] trailbase_sqlite::Error),
}
