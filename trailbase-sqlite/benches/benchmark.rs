#![allow(clippy::needless_return)]

pub mod connection;
pub mod error;

use criterion::{criterion_group, criterion_main, Bencher, Criterion, Throughput};
use log::*;
use parking_lot::Mutex;
use rand::Rng;
use rusqlite::types::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
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
  runtime: tokio::runtime::Runtime,
  conn: C,
}

impl<C: AsyncConnection> AsyncBenchmarkSetup<C> {
  fn setup(
    builder: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
  ) -> Result<Self, BenchmarkError> {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let fname = tmp_dir.path().join("main.sqlite");

    let runtime = tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()
      .unwrap();
    let conn = runtime.block_on(builder(fname))?;

    return Ok(Self {
      dir: tmp_dir,
      runtime,
      conn,
    });
  }

  fn fname(&self) -> PathBuf {
    return self.dir.path().join("main.sqlite");
  }
}

fn async_insert_benchmark<C: AsyncConnection + 'static>(
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

  let conn = Arc::new(setup.conn);

  const N: u64 = 100;
  b.to_async(&setup.runtime).iter_custom(async |iters: u64| {
    let start = Instant::now();

    let tasks = (0..iters).map(|i| {
      let conn = conn.clone();
      return setup.runtime.spawn(async move {
        for j in i * N..(i + 1) * N {
          conn
            .async_execute(
              format!("INSERT INTO 'table' (a) VALUES (?1)"),
              [Value::Integer(j as i64)],
            )
            .await
            .unwrap();
        }
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn insert_benchmark_group(c: &mut Criterion) {
  try_init_logger();

  info!("Running insertion benchmarks");

  let mut group = c.benchmark_group("Insert");
  group.measurement_time(Duration::from_secs(10));
  group.sample_size(500);
  group.throughput(Throughput::Elements(1));

  group.bench_function("trailbase-sqlite", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        None,
      )?);
    })
  });

  group.bench_function("locked-rusqlite", |b| {
    async_insert_benchmark(b, async |fname| {
      Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
        &fname,
      )?)))
    })
  });

  let id = std::sync::atomic::AtomicU64::new(0);
  group.bench_function("TL-rusqlite", |b| {
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

fn async_read_benchmark<C: AsyncConnection + 'static>(
  b: &mut Bencher,
  f: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
  fname: &Path,
  n: usize,
) {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let conn = Arc::new(runtime.block_on(f(fname.to_path_buf())).unwrap());

  b.to_async(&runtime).iter_custom(async |iters| {
    let start = Instant::now();

    let tasks = (0..iters).map(|i| {
      let conn = conn.clone();
      return runtime.spawn(async move {
        conn
          .async_query::<i64>(
            "SELECT id FROM 'read_table' WHERE id = ?1",
            [Value::Integer((i as usize % n) as i64)],
          )
          .await
          .unwrap()
      });
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn read_benchmark_group(c: &mut Criterion) {
  try_init_logger();

  info!("Running read benchmarks");

  let tmp_dir = tempfile::TempDir::new().unwrap();
  let fname = tmp_dir.path().join("main.sqlite");

  // Setup
  const N: usize = 20000;
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

  let mut group = c.benchmark_group("Read");
  group.measurement_time(Duration::from_secs(10));
  group.sample_size(500);
  group.throughput(Throughput::Elements(1));

  group.bench_function("trailbase-sqlite", |b| {
    async_read_benchmark(
      b,
      async |fname| {
        return Ok(Connection::new(
          || rusqlite::Connection::open(&fname),
          None,
        )?);
      },
      &fname.as_path(),
      N,
    )
  });

  group.bench_function("locked-rusqlite", |b| {
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
  group.bench_function("TL-rusqlite", |b| {
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

fn async_mixed_benchmark<C: AsyncConnection + 'static>(
  b: &mut Bencher,
  assets: Arc<AsyncBenchmarkSetup<C>>,
  n: i64,
) {
  async fn fast_read_query<C: AsyncConnection>(conn: &C, i: i64) {
    conn
      .async_query::<i64>("SELECT prop FROM 'A' WHERE id = ?1", [Value::Integer(i)])
      .await
      .unwrap();
  }

  async fn slow_read_query<C: AsyncConnection>(conn: &C, i: i64) {
    conn
      .async_query::<i64>(
        "SELECT A.id, B.id FROM A LEFT JOIN Bridge ON A.id = Bridge.a LEFT JOIN B ON Bridge.b = B.id WHERE A.id = ?1",
        [Value::Integer(i)],
      )
      .await
      .unwrap();
  }

  async fn write_query<C: AsyncConnection>(conn: &C) {
    conn
      .async_execute(
        "INSERT INTO 'write_table' (payload) VALUES (?1)",
        [Value::Blob([0; 256].into())],
      )
      .await
      .unwrap();
  }

  b.to_async(&assets.runtime).iter_custom(async |iters| {
    let mut rng = rand::rng();
    let start = Instant::now();

    let tasks = (0..iters).map(|i| {
      let assets_clone = assets.clone();
      return match i % 10 {
        0 | 6 => {
          let idx: i64 = rng.random_range(0..n);
          assets.runtime.spawn(async move {
            slow_read_query(&assets_clone.conn, idx).await;
          })
        }
        5 | 9 => assets.runtime.spawn(async move {
          write_query(&assets_clone.conn).await;
        }),
        _ => {
          let idx: i64 = rng.random_range(0..n);
          assets.runtime.spawn(async move {
            fast_read_query(&assets_clone.conn, idx).await;
          })
        }
      };
    });

    futures_util::future::join_all(tasks).await;

    return start.elapsed();
  });
}

fn mixed_benchmark_group(c: &mut Criterion) {
  try_init_logger();

  info!("Running mixed benchmarks");

  const N: i64 = 5000;

  fn setup<C: AsyncConnection>(
    builder: impl AsyncFnOnce(PathBuf) -> Result<C, BenchmarkError>,
  ) -> AsyncBenchmarkSetup<C> {
    let assets = AsyncBenchmarkSetup::setup(builder).unwrap();

    let conn = rusqlite::Connection::open(&assets.fname()).unwrap();
    conn
      .execute_batch(
        r#"
          CREATE TABLE 'write_table' (id INTEGER PRIMARY KEY NOT NULL, payload BLOB) STRICT;

          CREATE TABLE 'A' (id INTEGER PRIMARY KEY NOT NULL, prop INTEGER NOT NULL) STRICT;
          CREATE TABLE 'B' (id INTEGER PRIMARY KEY NOT NULL, prop INTEGER NOT NULL) STRICT;

          CREATE TABLE 'Bridge' (
            a INTEGER NOT NULL,  -- Technically 'REFERENCES A(prop)' but dodging unique/index requirement
            b INTEGER NOT NULL   -- Technically 'REFERENCES B(prop)' but dodging unique/index requirement
          ) STRICT;
        "#,
      )
      .unwrap();

    for i in 0..N {
      let exec = |query: &str| conn.execute(query, rusqlite::params!(i)).unwrap();

      exec("INSERT INTO 'A' (id, prop) VALUES (?1, ?1)");
      exec("INSERT INTO 'B' (id, prop) VALUES (?1, ?1)");
      exec("INSERT INTO 'Bridge' (a, b) VALUES (?1, ?1)");
    }

    return assets;
  }

  let mut group = c.benchmark_group("QueryMix");
  group.measurement_time(Duration::from_secs(10));
  group.sample_size(500);
  group.throughput(Throughput::Elements(1));

  {
    let assets = Arc::new(setup(async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        None,
      )?);
    }));

    group.bench_function("trailbase-sqlite", |b| {
      let assets = assets.clone();
      async_mixed_benchmark(b, assets, N);
    });
  }

  {
    let assets = Arc::new(setup(async |fname| {
      Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
        &fname,
      )?)))
    }));

    group.bench_function("locked-rusqlite", |b| {
      let assets = assets.clone();
      async_mixed_benchmark(b, assets, N);
    });
  }

  {
    let id = std::sync::atomic::AtomicU64::new(0);
    let assets = Arc::new(setup(async |fname| {
      let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      debug!("New ThreadLocalRusqlite: {id}");

      Ok(ThreadLocalRusqlite(
        Box::new(move || rusqlite::Connection::open(&fname).unwrap()),
        id,
      ))
    }));

    group.bench_function("TL-rusqlite", |b| {
      let assets = assets.clone();
      async_mixed_benchmark(b, assets, N);
    });
  }
}

criterion_group!(
  benches,
  insert_benchmark_group,
  read_benchmark_group,
  mixed_benchmark_group
);
criterion_main!(benches);
