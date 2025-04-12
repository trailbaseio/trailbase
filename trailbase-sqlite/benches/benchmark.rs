#![allow(clippy::needless_return)]

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use log::*;
use parking_lot::Mutex;
use rusqlite::types::{ToSql, Value};
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

/// Only meant for reference. This implementation is ill-suited since it can clog-up the tokio
/// runtime with sync sqlite calls.
struct SharedRusqlite(Mutex<rusqlite::Connection>);

impl AsyncConnection for SharedRusqlite {
  async fn async_execute(
    &self,
    sql: impl Into<String>,
    params: impl Into<Vec<Value>>,
  ) -> Result<(), BenchmarkError> {
    let params: Vec<Value> = params.into();
    let p: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();

    self.0.lock().execute(&sql.into(), p.as_slice())?;

    return Ok(());
  }
}

/// Only meant for reference. This implementation is ill-suited since it can clog-up the tokio
/// runtime with sync sqlite calls.
/// Additionally, the simple thread_local setup only allows for one connection at the time.
struct ThreadLocalRusqlite(Box<dyn (Fn() -> rusqlite::Connection)>, u64);

impl ThreadLocalRusqlite {
  #[inline]
  fn call<T>(
    &self,
    f: impl FnOnce(&mut rusqlite::Connection) -> rusqlite::Result<T>,
  ) -> rusqlite::Result<T> {
    use std::cell::{OnceCell, RefCell};
    thread_local! {
      static CELL : OnceCell<RefCell<(rusqlite::Connection, u64)>> = OnceCell::new();
    }

    return CELL.with(|cell| {
      fn init(s: &ThreadLocalRusqlite) -> (rusqlite::Connection, u64) {
        return (s.0(), s.1);
      }

      let ref_cell = cell.get_or_init(|| RefCell::new(init(self)));
      {
        let (conn, id): &mut (rusqlite::Connection, u64) = &mut ref_cell.borrow_mut();
        if *id == self.1 {
          return f(conn);
        }
      }

      // Reinitialize: new benchmark run with different DB folder.
      ref_cell.replace(init(self));
      let (conn, _): &mut (rusqlite::Connection, u64) = &mut ref_cell.borrow_mut();
      return f(conn);
    });
  }
}

impl AsyncConnection for ThreadLocalRusqlite {
  async fn async_execute(
    &self,
    sql: impl Into<String>,
    params: impl Into<Vec<Value>>,
  ) -> Result<(), BenchmarkError> {
    let params: Vec<Value> = params.into();
    let p: Vec<&dyn ToSql> = params.iter().map(|v| v as &dyn ToSql).collect();

    self.call(move |conn| conn.execute(&sql.into(), p.as_slice()))?;
    return Ok(());
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

  debug!("Set up: {:?}", setup.dir.path());

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

fn try_init_logger() {
  let _ = env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
    .format_timestamp_micros()
    .try_init();
}

fn insert_benchmarks_group(c: &mut Criterion) {
  try_init_logger();

  info!("Running insertion benchmarks");

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

  c.bench_function("shared/locked rusqlite insert", |b| {
    async_insert_benchmark(b, async |fname| {
      Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
        &fname,
      )?)))
    })
  });

  let id = std::sync::atomic::AtomicU64::new(0);
  c.bench_function("TL/pool rusqlite insert", |b| {
    async_insert_benchmark(b, async |fname| {
      let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      debug!("New ThreadLocalRusqlite: {id}");

      Ok(ThreadLocalRusqlite(
        Box::new(move || rusqlite::Connection::open(&fname).unwrap()),
        id,
      ))
    })
  });
}

criterion_group!(benches, insert_benchmarks_group);

criterion_main!(benches);
