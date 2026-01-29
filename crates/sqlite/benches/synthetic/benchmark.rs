#![allow(clippy::needless_return)]

pub mod connection;
pub mod error;

use criterion::{Bencher, Criterion, Throughput, criterion_group, criterion_main};
use log::*;
use parking_lot::Mutex;
use rand::Rng;
use rusqlite::types::Value;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use trailbase_sqlite::connection::{Connection, Options};

use crate::connection::{AsyncConnection, SharedRusqlite, ThreadLocalRusqlite};
use crate::error::BenchmarkError;

fn try_init_logger() {
  let _ = env_logger::Builder::from_env(env_logger::Env::new().default_filter_or("info"))
    .format_timestamp_micros()
    .try_init();
}

fn build_runtime() -> tokio::runtime::Runtime {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let n = std::thread::available_parallelism().unwrap().get();
  if n != runtime.metrics().num_workers() {
    panic!(
      "expected {n} workers, got {}",
      runtime.metrics().num_workers()
    );
  }

  return runtime;
}

struct AsyncBenchmarkSetup<C: AsyncConnection> {
  #[allow(unused)]
  dir: tempfile::TempDir, // with RAII cleanup.
  fname: PathBuf,
  conn: C,
}

impl<C: AsyncConnection> AsyncBenchmarkSetup<C> {
  async fn setup(
    builder: impl AsyncFn(PathBuf) -> Result<C, BenchmarkError>,
  ) -> Result<Self, BenchmarkError> {
    let tmp_dir = tempfile::TempDir::new().unwrap();
    let fname = tmp_dir.path().join("main.sqlite");
    let conn = builder(fname.clone()).await?;

    debug!("Set up: {fname:?}");

    return Ok(Self {
      dir: tmp_dir,
      fname,
      conn,
    });
  }
}

fn async_insert_benchmark<C: AsyncConnection + 'static>(
  b: &mut Bencher,
  builder: impl AsyncFn(PathBuf) -> Result<C, BenchmarkError> + Clone,
) {
  let runtime = build_runtime();

  const N: u64 = 100;
  b.to_async(&runtime).iter_custom(async |iters: u64| {
    // NOTE: create new DB every time to avoid hysteresis by appending to a single DB across runs.
    let setup = AsyncBenchmarkSetup::<C>::setup(builder.clone())
      .await
      .unwrap();
    setup
      .conn
      .async_execute("CREATE TABLE 'table' (a  INTEGER) STRICT", [])
      .await
      .unwrap();
    let conn = Arc::new(setup.conn);

    let start = Instant::now();

    let tasks = (0..iters).map(|i| {
      let conn = conn.clone();
      return runtime.spawn(async move {
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
  group.sample_size(200);
  group.throughput(Throughput::Elements(1));

  group.bench_function("trailbase-sqlite (1 thread)", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        None,
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (2 threads)", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 2,
          ..Default::default()
        }),
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (4 threads)", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 4,
          ..Default::default()
        }),
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (8 threads)", |b| {
    async_insert_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 8,
          ..Default::default()
        }),
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
  builder: impl AsyncFn(PathBuf) -> Result<C, BenchmarkError> + Clone,
) {
  let runtime = build_runtime();

  // NOTE: Here we don't have to create a new DB every time, since it's read-only.
  let setup = runtime
    .block_on(AsyncBenchmarkSetup::<C>::setup(builder.clone()))
    .unwrap();

  // Setup
  const N: i64 = 20000;
  {
    let conn = rusqlite::Connection::open(setup.fname).unwrap();
    conn
      .execute(
        "CREATE TABLE 'read_table' (id INTEGER PRIMARY KEY NOT NULL) STRICT",
        [],
      )
      .unwrap();

    for i in 0..N {
      conn
        .execute(
          "INSERT INTO 'read_table' (id) VALUES (?1)",
          [Value::Integer(i)],
        )
        .unwrap();
    }
  }

  let conn = Arc::new(setup.conn);

  b.to_async(&runtime).iter_custom(async |iters| {
    let mut rng = rand::rng();
    let start = Instant::now();

    let tasks = (0..iters).map(|_| {
      let idx: i64 = rng.random_range(0..N);
      let conn = conn.clone();
      return runtime.spawn(async move {
        conn
          .async_read_query::<i64>(
            "SELECT id FROM 'read_table' WHERE id = ?1",
            [Value::Integer(idx)],
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

  let mut group = c.benchmark_group("Read");
  group.measurement_time(Duration::from_secs(2));
  group.sample_size(100);
  group.throughput(Throughput::Elements(1));

  group.bench_function("trailbase-sqlite (1 thread)", |b| {
    async_read_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        None,
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (2 threads)", |b| {
    async_read_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 2,
          ..Default::default()
        }),
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (4 threads)", |b| {
    async_read_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 4,
          ..Default::default()
        }),
      )?);
    })
  });

  group.bench_function("trailbase-sqlite (8 threads)", |b| {
    async_read_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 8,
          ..Default::default()
        }),
      )?);
    })
  });

  group.bench_function("locked-rusqlite", |b| {
    async_read_benchmark(b, async |fname| {
      Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
        &fname,
      )?)))
    })
  });

  let id = std::sync::atomic::AtomicU64::new(0);
  group.bench_function("TL-rusqlite", |b| {
    async_read_benchmark(b, async |fname| {
      let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      debug!("New ThreadLocalRusqlite: {id}");

      Ok(ThreadLocalRusqlite(
        Box::new(move || rusqlite::Connection::open(&fname).unwrap()),
        id,
      ))
    })
  });
}

fn async_mixed_benchmark<C: AsyncConnection + 'static>(
  b: &mut Bencher,
  builder: impl AsyncFn(PathBuf) -> Result<C, BenchmarkError> + Clone,
) {
  async fn fast_read_query<C: AsyncConnection>(conn: &C, i: i64) {
    conn
      .async_read_query::<i64>("SELECT prop FROM 'A' WHERE id = ?1", [Value::Integer(i)])
      .await
      .unwrap();
  }

  async fn slow_read_query<C: AsyncConnection>(conn: &C, i: i64) {
    conn
      .async_read_query::<i64>(
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

  let runtime = build_runtime();
  b.to_async(&runtime).iter_custom(async |iters| {
    const N: i64 = 5000;
    let setup = AsyncBenchmarkSetup::<C>::setup(builder.clone()).await.unwrap();

    {
      let conn = rusqlite::Connection::open(&setup.fname).unwrap();
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
    }

    let mut rng = rand::rng();
    let conn = Arc::new(setup.conn);
    let start = Instant::now();

    let tasks = (0..iters).map(|i| {
      let conn = conn.clone();
      return match i % 10 {
        // NOTE: put slow queries at low `i` so that criterion, when estimating the necessary
        // iterations picks a conservative estimate.
        0 | 5 => runtime.spawn(async move {
          write_query(&*conn).await;
        }),
        1 | 6 => {
          let idx: i64 = rng.random_range(0..N);
          runtime.spawn(async move {
            slow_read_query(&*conn, idx).await;
          })
        }
        _ => {
          let idx: i64 = rng.random_range(0..N);
          runtime.spawn(async move {
            fast_read_query(&*conn, idx).await;
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

  let mut group = c.benchmark_group("QueryMix");
  group.measurement_time(Duration::from_secs(10));
  group.sample_size(200);
  group.throughput(Throughput::Elements(1));

  group.bench_function("trailbase-sqlite (1 thread)", |b| {
    async_mixed_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        None,
      )?);
    });
  });

  group.bench_function("trailbase-sqlite (2 threads)", |b| {
    async_mixed_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 2,
          ..Default::default()
        }),
      )?);
    });
  });

  group.bench_function("trailbase-sqlite (4 threads)", |b| {
    async_mixed_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 4,
          ..Default::default()
        }),
      )?);
    });
  });

  group.bench_function("trailbase-sqlite (8 threads)", |b| {
    async_mixed_benchmark(b, async |fname| {
      return Ok(Connection::new(
        || rusqlite::Connection::open(&fname),
        Some(Options {
          n_read_threads: 8,
          ..Default::default()
        }),
      )?);
    });
  });

  group.bench_function("locked-rusqlite", |b| {
    async_mixed_benchmark(b, async |fname| {
      Ok(SharedRusqlite(Mutex::new(rusqlite::Connection::open(
        &fname,
      )?)))
    });
  });

  {
    let id = std::sync::atomic::AtomicU64::new(0);
    group.bench_function("TL-rusqlite", |b| {
      async_mixed_benchmark(b, async |fname| {
        let id = id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        debug!("New ThreadLocalRusqlite: {id}");

        Ok(ThreadLocalRusqlite(
          Box::new(move || rusqlite::Connection::open(&fname).unwrap()),
          id,
        ))
      });
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
