#![allow(clippy::needless_return, async_fn_in_trait)]

pub mod connection;
pub mod error;

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use log::*;
use parking_lot::Mutex;
use rusqlite::types::Value;
use std::path::{Path, PathBuf};
use std::time::Instant;
use trailbase_sqlite::Connection;

use crate::connection::{AsyncConnection, SharedRusqlite, ThreadLocalRusqlite};
use crate::error::BenchmarkError;

fn try_init_logger() {
  let _ = env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
    .format_timestamp_micros()
    .try_init();
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

fn insert_benchmark_group(c: &mut Criterion) {
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

fn async_read_benchmark<C: AsyncConnection>(
  b: &mut Bencher,
  f: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
  fname: &Path,
  n: usize,
) {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let conn = runtime.block_on(f(fname.to_path_buf())).unwrap();

  b.to_async(&runtime).iter(async || {
    for i in 0..n {
      let _a: i64 = conn
        .async_query(
          "SELECT id FROM 'read_table' WHERE id = ?1",
          [Value::Integer(i as i64)],
        )
        .await
        .unwrap();
    }
  });
}

fn read_benchmark_group(c: &mut Criterion) {
  try_init_logger();

  info!("Running read benchmarks");

  let tmp_dir = tempfile::TempDir::new().unwrap();
  let fname = tmp_dir.path().join("main.sqlite");

  // Setup
  const N: usize = 5000;
  {
    let conn = rusqlite::Connection::open(&fname).unwrap();
    conn
      .execute(
        "CREATE TABLE 'read_table' (id INTEGER PRIMARY KEY NOT NULL) STRICT",
        (),
      )
      .unwrap();
    for i in 0..N {
      conn
        .execute(
          "INSERT INTO 'read_table' (id) VALUES (?1)",
          rusqlite::params!(i),
        )
        .unwrap();
    }
  }

  c.bench_function("trailbase-sqlite read", |b| {
    async_read_benchmark(
      b,
      async |fname| {
        return Ok(Connection::new(
          || {
            return rusqlite::Connection::open(&fname);
          },
          None,
        )?);
      },
      &fname.as_path(),
      N,
    )
  });

  c.bench_function("shared/locked rusqlite read", |b| {
    async_read_benchmark(
      b,
      async |fname| {
        Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
          &fname,
        )?)))
      },
      &fname.as_path(),
      N,
    )
  });

  let id = std::sync::atomic::AtomicU64::new(0);
  c.bench_function("TL/pool rusqlite read", |b| {
    async_read_benchmark(
      b,
      async |fname| {
        let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        debug!("New ThreadLocalRusqlite: {id}");

        Ok(ThreadLocalRusqlite(
          Box::new(move || rusqlite::Connection::open(&fname).unwrap()),
          id,
        ))
      },
      &fname.as_path(),
      N,
    )
  });
}

criterion_group!(benches, insert_benchmark_group, read_benchmark_group);
criterion_main!(benches);
