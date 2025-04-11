#![allow(clippy::needless_return)]

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use log::*;
use rusqlite::types::Value;
use std::path::PathBuf;
use std::time::Instant;
use trailbase_sqlite::Connection;

#[derive(thiserror::Error, Debug)]
#[allow(unused)]
enum BenchmarkError {
  #[error("Other error: {0}")]
  Other(Box<dyn std::error::Error + Sync + Send>),

  #[error("Rusqlite error: {0}")]
  Rusqlite(#[from] rusqlite::Error),

  #[error("TrailBase error: {0}")]
  TrailBase(#[from] trailbase_sqlite::Error),
}

trait AsyncConnection {
  // async fn query<T: FromSql + Send + 'static>(
  //   &self,
  //   sql: &str,
  //   params: Vec<Value>,
  // ) -> Result<T, BenchmarkError>;

  async fn async_execute(
    &self,
    sql: impl Into<String>,
    params: impl Into<Vec<Value>>,
  ) -> Result<(), BenchmarkError>;
}

impl AsyncConnection for Connection {
  // async fn query<T: FromSql + Send + 'static>(
  //   &self,
  //   sql: &str,
  //   params: Vec<Value>,
  // ) -> Result<T, BenchmarkError> {
  //   let sql = sql.to_string();
  //   return Ok(
  //     self
  //       .call(
  //         move |conn: &mut rusqlite::Connection| -> Result<_, trailbase_sqlite::Error> {
  //           let mut stmt = conn.prepare_cached(&sql)?;
  //           for (idx, v) in params.into_iter().enumerate() {
  //             stmt.raw_bind_parameter(idx+1, v)?;
  //           }
  //           let mut rows = stmt.raw_query();
  //           if let Ok(Some(row)) = rows.next() {
  //             return Ok(row.get::<_, T>(0)?);
  //           }
  //
  //           return Err(rusqlite::Error::QueryReturnedNoRows.into());
  //         },
  //       )
  //       .await?,
  //   );
  // }

  async fn async_execute(
    &self,
    sql: impl Into<String>,
    params: impl Into<Vec<Value>>,
  ) -> Result<(), BenchmarkError> {
    let sql: String = sql.into();
    let params: Vec<Value> = params.into();
    return Ok(
      self
        .call(
          move |conn: &mut rusqlite::Connection| -> Result<_, trailbase_sqlite::Error> {
            let mut stmt = conn.prepare_cached(&sql)?;
            for (idx, v) in params.into_iter().enumerate() {
              stmt.raw_bind_parameter(idx + 1, v)?;
            }
            let _ = stmt.raw_execute();

            return Ok(());
          },
        )
        .await?,
    );
  }
}

struct AsyncBenchmarkSetup<C: AsyncConnection> {
  #[allow(unused)]
  dir: tempfile::TempDir, // with RAII cleanup.
  conn: C,
  runtime: tokio::runtime::Runtime,
}

impl<C: AsyncConnection> AsyncBenchmarkSetup<C> {
  fn setup(
    f: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
  ) -> Result<Self, BenchmarkError> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()
      .unwrap();

    let tmp_dir = tempfile::TempDir::new().unwrap();
    let fname = tmp_dir.path().join("main.sqlite");

    return Ok(Self {
      dir: tmp_dir,
      conn: runtime.block_on(f(fname))?,
      runtime,
    });
  }
}

fn async_insert_benchmark<C: AsyncConnection>(
  b: &mut Bencher,
  f: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
) {
  let setup = AsyncBenchmarkSetup::setup(f).unwrap();

  setup
    .runtime
    .block_on(
      setup
        .conn
        .async_execute("CREATE TABLE 'table' (a  INTEGER) STRICT", []),
    )
    .unwrap();

  b.to_async(&setup.runtime).iter_custom(async |iter: u64| {
    const N: u64 = 100;

    let start = Instant::now();
    for i in 0..iter {
      for j in i * N..(i + 1) * N {
        setup
          .conn
          .async_execute(
            format!("INSERT INTO 'table' (a) VALUES (?1)"),
            [Value::Integer(j as i64)],
          )
          .await
          .unwrap();
      }
    }

    return start.elapsed();
  });
}

fn all_benchmarks_group(c: &mut Criterion) {
  env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
    .format_timestamp_micros()
    .init();

  info!("Running benchmarks");

  c.bench_function("trailbase-sqlite insert", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || {
          return rusqlite::Connection::open(&fname);
        },
        None,
      )?);
    })
  });
}

criterion_group!(benches, all_benchmarks_group);
criterion_main!(benches);
