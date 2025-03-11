use chrono::{DateTime, Duration, Utc};
use cron::Schedule;
use futures_util::future::BoxFuture;
use log::*;
use parking_lot::Mutex;
use std::collections::{hash_map::Entry, HashMap};
use std::future::Future;
use std::str::FromStr;
use std::sync::{
  atomic::{AtomicI32, Ordering},
  Arc,
};
use trailbase_sqlite::{params, Connection};

use crate::config::proto::{Config, SystemCronJob, SystemCronJobId};
use crate::constants::{DEFAULT_REFRESH_TOKEN_TTL, LOGS_RETENTION_DEFAULT, SESSION_TABLE};
use crate::DataDir;

type CallbackError = Box<dyn std::error::Error + Sync + Send>;
type CallbackFunction = dyn Fn() -> BoxFuture<'static, Result<(), CallbackError>> + Sync + Send;
type LatestCallbackExecution = Option<(DateTime<Utc>, Option<CallbackError>)>;

static TASK_COUNTER: AtomicI32 = AtomicI32::new(1024);

pub trait CallbackResultTrait {
  fn into_result(self) -> Result<(), CallbackError>;
}

impl CallbackResultTrait for () {
  fn into_result(self) -> Result<(), CallbackError> {
    return Ok(());
  }
}

impl<T: Into<CallbackError>> CallbackResultTrait for Result<(), T> {
  fn into_result(self) -> Result<(), CallbackError> {
    return self.map_err(|e| e.into());
  }
}

#[allow(unused)]
pub struct Task {
  pub id: i32,
  pub name: String,
  pub schedule: Schedule,
  pub(crate) callback: Arc<CallbackFunction>,

  handle: Option<tokio::task::AbortHandle>,
  latest: Arc<Mutex<LatestCallbackExecution>>,
}

pub struct TaskRegistry {
  pub(crate) tasks: Mutex<HashMap<i32, Task>>,
}

impl Task {
  fn new(id: i32, name: String, schedule: Schedule, callback: Arc<CallbackFunction>) -> Self {
    return Task {
      id,
      name,
      schedule,
      callback,
      handle: None,
      latest: Arc::new(Mutex::new(None)),
    };
  }

  fn start(&mut self) {
    let name = self.name.clone();
    let callback = self.callback.clone();
    let schedule = self.schedule.clone();
    let latest = self.latest.clone();

    let handle = tokio::spawn(async move {
      loop {
        let now = Utc::now();
        let Some(next) = schedule.upcoming(Utc).next() else {
          break;
        };
        let Ok(duration) = (next - now).to_std() else {
          log::warn!("Invalid duration for '{name}': {next:?}");
          continue;
        };

        tokio::time::sleep(duration).await;

        let result = (*callback)().await;
        *latest.lock() = Some((Utc::now(), result.err()));
      }

      log::info!("Exited task: '{name}'");
    });

    self.handle = Some(handle.abort_handle());
  }

  async fn run_now(&self) -> Result<(), CallbackError> {
    return (self.callback)().await;
  }

  fn stop(&mut self) {
    if let Some(ref handle) = self.handle {
      handle.abort();
    }
    self.handle = None;
  }
}

impl TaskRegistry {
  pub fn new() -> Self {
    return TaskRegistry {
      tasks: Mutex::new(HashMap::new()),
    };
  }

  pub fn add_task(
    &self,
    id: Option<i32>,
    name: impl Into<String>,
    schedule: Schedule,
    callback: Box<CallbackFunction>,
  ) -> bool {
    let id = id.unwrap_or_else(|| TASK_COUNTER.fetch_add(1, Ordering::SeqCst));
    return match self.tasks.lock().entry(id) {
      Entry::Occupied(_) => false,
      Entry::Vacant(entry) => {
        let task = {
          let mut task = Task::new(id, name.into(), schedule, callback.into());
          task.start();
          task
        };

        entry.insert(task);

        true
      }
    };
  }
}

impl Drop for TaskRegistry {
  fn drop(&mut self) {
    let mut tasks = self.tasks.lock();
    for t in tasks.values_mut() {
      t.stop();
    }
  }
}

pub fn build_callback<O, F, Fut>(f: F) -> Box<CallbackFunction>
where
  F: 'static + Sync + Send + Fn() -> Fut,
  Fut: Sync + Send + Future<Output = O>,
  O: CallbackResultTrait,
{
  let fun = Arc::new(f);

  return Box::new(move || {
    let fun = fun.clone();

    return Box::pin(async move {
      return fun().await.into_result();
    });
  });
}

struct Job {
  name: &'static str,
  default: SystemCronJob,
  callback: Box<CallbackFunction>,
}

fn build_job(
  id: SystemCronJobId,
  data_dir: &DataDir,
  config: &Config,
  conn: &Connection,
  logs_conn: &Connection,
) -> Job {
  return match id {
    SystemCronJobId::Undefined => Job {
      name: "",
      default: SystemCronJob::default(),
      #[allow(unreachable_code)]
      callback: build_callback(move || {
        panic!("undefined cron job");
        async {}
      }),
    },
    SystemCronJobId::Backup => {
      let backup_file = data_dir.backup_path().join("backup.db");
      let conn = conn.clone();

      Job {
        name: "Backup",
        default: SystemCronJob {
          id: Some(id as i32),
          spec: Some("@daily".into()),
          disable_job: Some(true),
        },
        callback: build_callback(move || {
          let conn = conn.clone();
          let backup_file = backup_file.clone();

          return async move {
            conn
              .call(|conn| {
                return Ok(conn.backup(
                  rusqlite::DatabaseName::Main,
                  backup_file,
                  /* progress= */ None,
                )?);
              })
              .await
              .map_err(|err| {
                error!("Backup failed: {err}");
                err
              })?;

            Ok::<(), trailbase_sqlite::Error>(())
          };
        }),
      }
    }
    SystemCronJobId::Heartbeat => Job {
      name: "Heartbeat",
      default: SystemCronJob {
        id: Some(id as i32),
        //         sec  min   hour   day of month   month   day of week  year
        spec: Some("17   *     *         *            *         *         *".into()),
        disable_job: Some(false),
      },
      callback: build_callback(|| async {
        info!("alive");
      }),
    },
    SystemCronJobId::LogCleaner => {
      let logs_conn = logs_conn.clone();
      let retention = config
        .server
        .logs_retention_sec
        .map_or(LOGS_RETENTION_DEFAULT, Duration::seconds);

      Job {
        name: "Logs Cleanup",
        default: SystemCronJob {
          id: Some(id as i32),
          spec: Some("@hourly".into()),
          disable_job: Some(false),
        },
        callback: build_callback(move || {
          let logs_conn = logs_conn.clone();

          return async move {
            let timestamp = (Utc::now() - retention).timestamp();
            logs_conn
              .execute("DELETE FROM _logs WHERE created < $1", params!(timestamp))
              .await
              .map_err(|err| {
                warn!("Periodic logs cleanup failed: {err}");
                err
              })?;

            Ok::<(), trailbase_sqlite::Error>(())
          };
        }),
      }
    }
    SystemCronJobId::AuthCleaner => {
      let user_conn = conn.clone();
      let refresh_token_ttl = config
        .auth
        .refresh_token_ttl_sec
        .map_or(DEFAULT_REFRESH_TOKEN_TTL, Duration::seconds);

      Job {
        name: "Auth Cleanup",
        default: SystemCronJob {
          id: Some(id as i32),
          spec: Some("@hourly".into()),
          disable_job: Some(false),
        },
        callback: build_callback(move || {
          let user_conn = user_conn.clone();

          return async move {
            let timestamp = (Utc::now() - refresh_token_ttl).timestamp();

            user_conn
              .execute(
                &format!("DELETE FROM '{SESSION_TABLE}' WHERE updated < $1"),
                params!(timestamp),
              )
              .await
              .map_err(|err| {
                warn!("Periodic session cleanup failed: {err}");
                err
              })?;

            Ok::<(), trailbase_sqlite::Error>(())
          };
        }),
      }
    }
    SystemCronJobId::QueryOptimizer => {
      let conn = conn.clone();

      Job {
        name: "Query Optimizer",
        default: SystemCronJob {
          id: Some(id as i32),
          spec: Some("@daily".into()),
          disable_job: Some(false),
        },
        callback: build_callback(move || {
          let conn = conn.clone();

          return async move {
            conn.execute("PRAGMA optimize", ()).await.map_err(|err| {
              warn!("Periodic query optimizer failed: {err}");
              return err;
            })?;

            Ok::<(), trailbase_sqlite::Error>(())
          };
        }),
      }
    }
  };
}

pub fn build_task_registry_from_config(
  config: &Config,
  data_dir: &DataDir,
  conn: &Connection,
  logs_conn: &Connection,
) -> Result<TaskRegistry, CallbackError> {
  let jobs = [
    SystemCronJobId::Backup,
    SystemCronJobId::Heartbeat,
    SystemCronJobId::LogCleaner,
    SystemCronJobId::AuthCleaner,
    SystemCronJobId::QueryOptimizer,
  ];

  let tasks = TaskRegistry::new();
  for job_id in jobs {
    let Job {
      name,
      default,
      callback,
    } = build_job(job_id, data_dir, config, conn, logs_conn);

    let config = config
      .cron
      .system_jobs
      .iter()
      .find(|j| j.id == Some(job_id as i32))
      .unwrap_or(&default);

    if config.disable_job == Some(true) {
      log::debug!("Job '{name}' disabled. Skipping");
      continue;
    }

    let spec = config
      .spec
      .as_ref()
      .unwrap_or_else(|| default.spec.as_ref().expect("startup"));

    match Schedule::from_str(spec) {
      Ok(schedule) => {
        let success = tasks.add_task(Some(job_id as i32), name, schedule, callback);
        if !success {
          log::error!("Duplicate job definition for '{name}'");
        }
      }
      Err(err) => {
        log::error!("Invalid time spec for '{name}': {err}");
      }
    };
  }

  return Ok(tasks);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_cron() {
    //               sec      min   hour   day of month   month   day of week  year
    let expression = "*/100   *     *         *            *         *          *";
    assert!(Schedule::from_str(expression).is_err());

    let expression = "*/40    *     *         *            *         *          *";
    Schedule::from_str(expression).unwrap();
  }

  #[tokio::test]
  async fn test_scheduler() {
    // NOTE: Cron is time and not interval based, i.e. something like every 100s is not
    // representable in a single cron spec. Make sure that our cron parser detects that proerly,
    // e.g. croner does not and will just produce buggy intervals.
    // NOTE: Interval-based scheduling is generally problematic because it drifts. For something
    // like a backup you certainly want to control when and not how often it happens (e.g. at
    // night).
    let registry = TaskRegistry::new();

    let (sender, receiver) = async_channel::unbounded::<()>();

    //               sec  min   hour   day of month   month   day of week  year
    let expression = "*    *     *         *            *         *         *";
    registry.add_task(
      None,
      "Test Task",
      Schedule::from_str(expression).unwrap(),
      build_callback(move || {
        let sender = sender.clone();
        return async move {
          sender.send(()).await.unwrap();
          Err("result")
        };
      }),
    );

    receiver.recv().await.unwrap();

    let tasks = registry.tasks.lock();
    let first = tasks.keys().next().unwrap();

    let latest = tasks.get(first).unwrap().latest.lock();
    let (_timestamp, err) = latest.as_ref().unwrap();
    assert_eq!(err.as_ref().unwrap().to_string(), "result");
  }
}
