use chrono::{DateTime, Duration, Utc};
use cron::Schedule;
use futures_util::future::BoxFuture;
use log::*;
use parking_lot::Mutex;
use std::collections::{HashMap, hash_map::Entry};
use std::future::Future;
use std::str::FromStr;
use std::sync::{
  Arc,
  atomic::{AtomicI32, Ordering},
};
use trailbase_sqlite::{Connection, params};

use crate::DataDir;
use crate::app_state::ObjectStore;
use crate::config::proto::{Config, SystemJob, SystemJobId};
use crate::connection::ConnectionManager;
use crate::constants::{DEFAULT_REFRESH_TOKEN_TTL, LOGS_RETENTION_DEFAULT, SESSION_TABLE};
use crate::records::files::{FileDeletionsDb, FileError, delete_pending_files_impl};

type CallbackError = Box<dyn std::error::Error + Sync + Send>;
type CallbackFunction = dyn Fn() -> BoxFuture<'static, Result<(), CallbackError>> + Sync + Send;

pub struct ExecutionResult {
  pub start_time: DateTime<Utc>,
  pub end_time: DateTime<Utc>,
  pub error: Option<CallbackError>,
}

static JOB_ID_COUNTER: AtomicI32 = AtomicI32::new(1024);

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

struct JobState {
  name: String,

  schedule: Schedule,
  callback: Arc<CallbackFunction>,

  handle: Option<tokio::task::AbortHandle>,
  latest: Option<ExecutionResult>,
}

#[derive(Clone)]
pub struct Job {
  pub id: i32,
  state: Arc<Mutex<JobState>>,
}

impl Job {
  fn new(id: i32, name: String, schedule: Schedule, callback: Box<CallbackFunction>) -> Self {
    return Job {
      id,
      state: Arc::new(Mutex::new(JobState {
        name,
        schedule,
        callback: callback.into(),
        handle: None,
        latest: None,
      })),
    };
  }

  pub fn start(&self) {
    let job = self.clone();
    let (name, schedule) = {
      let lock = job.state.lock();
      if let Some(ref handle) = lock.handle {
        warn!("starting an already running job");
        handle.abort();
      }

      (lock.name.clone(), lock.schedule.clone())
    };

    self.state.lock().handle = Some(
      tokio::spawn(async move {
        loop {
          let Some(next) = schedule.upcoming(Utc).next() else {
            break;
          };
          let Ok(duration) = (next - Utc::now()).to_std() else {
            warn!("Invalid duration for '{name}': {next:?}");
            continue;
          };

          tokio::time::sleep(duration).await;

          let _ = job.run_now().await;
        }

        info!("Exited job: '{name}'");
      })
      .abort_handle(),
    );
  }

  async fn run_now(&self) -> Result<(), String> {
    let callback = self.state.lock().callback.clone();

    let start_time = Utc::now();
    let result = callback().await;
    let end_time = Utc::now();

    let result_str = result.as_ref().map_err(|err| err.to_string()).copied();
    self.state.lock().latest = Some(ExecutionResult {
      start_time,
      end_time,
      error: result.err(),
    });
    return result_str;
  }

  pub fn next_run(&self) -> Option<DateTime<Utc>> {
    let lock = self.state.lock();
    if lock.handle.is_some() {
      return lock.schedule.upcoming(Utc).next();
    }
    return None;
  }

  fn stop(&self) {
    let mut lock = self.state.lock();
    if let Some(ref handle) = lock.handle {
      handle.abort();
    }
    lock.handle = None;
  }

  pub fn running(&self) -> bool {
    return self.state.lock().handle.is_some();
  }

  pub fn latest(&self) -> Option<(DateTime<Utc>, Duration, Option<String>)> {
    if let Some(ref result) = self.state.lock().latest {
      return Some((
        result.start_time,
        result.end_time - result.start_time,
        result.error.as_ref().map(|err| err.to_string()),
      ));
    }
    return None;
  }

  pub fn name(&self) -> String {
    return self.state.lock().name.clone();
  }

  pub fn schedule(&self) -> Schedule {
    return self.state.lock().schedule.clone();
  }
}

pub struct JobRegistry {
  pub(crate) jobs: Mutex<HashMap<i32, Job>>,
}

impl JobRegistry {
  pub fn new() -> Self {
    return JobRegistry {
      jobs: Mutex::new(HashMap::new()),
    };
  }

  pub fn new_job(
    &self,
    id: Option<i32>,
    name: impl Into<String>,
    schedule: Schedule,
    callback: Box<CallbackFunction>,
  ) -> Option<Job> {
    let id = id.unwrap_or_else(|| JOB_ID_COUNTER.fetch_add(1, Ordering::SeqCst));
    return match self.jobs.lock().entry(id) {
      Entry::Occupied(_) => None,
      Entry::Vacant(entry) => Some(
        entry
          .insert(Job::new(id, name.into(), schedule, callback))
          .clone(),
      ),
    };
  }

  pub async fn run_job(&self, id: i32) -> Option<Result<(), String>> {
    let job = {
      let jobs = self.jobs.lock();
      jobs.get(&id)?.clone()
    };

    debug!("Running job {id}: {}", job.name());
    return Some(job.run_now().await);
  }
}

impl Drop for JobRegistry {
  fn drop(&mut self) {
    let mut jobs = self.jobs.lock();
    for t in jobs.values_mut() {
      t.stop();
    }
  }
}

pub fn build_callback<O, F, Fut>(f: F) -> Box<CallbackFunction>
where
  F: 'static + Sync + Send + Fn() -> Fut,
  Fut: Send + Future<Output = O>,
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

struct DefaultSystemJob {
  name: &'static str,
  default: SystemJob,
  callback: Box<CallbackFunction>,
}

fn build_job(
  id: SystemJobId,
  data_dir: &DataDir,
  config: &Config,
  connection_manager: &ConnectionManager,
  logs_conn: &Connection,
  object_store: Arc<ObjectStore>,
) -> DefaultSystemJob {
  return match id {
    SystemJobId::Undefined => DefaultSystemJob {
      name: "",
      default: SystemJob::default(),
      #[allow(unreachable_code)]
      callback: build_callback(move || {
        panic!("undefined job");
        async {}
      }),
    },
    SystemJobId::Backup => {
      let backup_file = data_dir.backup_path().join("backup.db");
      let main_conn = connection_manager.main_entry().connection.clone();

      DefaultSystemJob {
        name: "Backup",
        default: SystemJob {
          id: Some(id as i32),
          schedule: Some("@daily".into()),
          disabled: Some(true),
        },
        callback: build_callback(move || {
          let conn = main_conn.clone();
          let backup_file = backup_file.clone();

          return async move {
            conn
              .call(|conn| {
                return Ok(conn.backup("main", backup_file, /* progress= */ None)?);
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
    SystemJobId::Heartbeat => DefaultSystemJob {
      name: "Heartbeat",
      default: SystemJob {
        id: Some(id as i32),
        // sec   min   hour   day of month   month   day of week   year
        schedule: Some("17 * * * * * *".into()),
        disabled: Some(false),
      },
      callback: build_callback(|| async {
        debug!("alive");
      }),
    },
    SystemJobId::LogCleaner => {
      let logs_conn = logs_conn.clone();
      let retention = config
        .server
        .logs_retention_sec
        .map_or(LOGS_RETENTION_DEFAULT, Duration::seconds);

      DefaultSystemJob {
        name: "Logs Cleanup",
        default: SystemJob {
          id: Some(id as i32),
          schedule: Some("@hourly".into()),
          disabled: Some(false),
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
    SystemJobId::AuthCleaner => {
      let main_conn = connection_manager.main_entry().connection.clone();
      let refresh_token_ttl = config
        .auth
        .refresh_token_ttl_sec
        .map_or(DEFAULT_REFRESH_TOKEN_TTL, Duration::seconds);

      DefaultSystemJob {
        name: "Auth Cleanup",
        default: SystemJob {
          id: Some(id as i32),
          schedule: Some("@hourly".into()),
          disabled: Some(false),
        },
        callback: build_callback(move || {
          let user_conn = main_conn.clone();

          return async move {
            let timestamp = (Utc::now() - refresh_token_ttl).timestamp();

            user_conn
              .execute(
                format!("DELETE FROM '{SESSION_TABLE}' WHERE updated < $1"),
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
    SystemJobId::QueryOptimizer => {
      let main_conn = connection_manager.main_entry().connection.clone();

      DefaultSystemJob {
        name: "Query Optimizer",
        default: SystemJob {
          id: Some(id as i32),
          schedule: Some("@daily".into()),
          disabled: Some(false),
        },
        callback: build_callback(move || {
          let conn = main_conn.clone();

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
    SystemJobId::FileDeletions => {
      let connection_manager = connection_manager.clone();
      let databases = config.databases.clone();

      DefaultSystemJob {
        name: "File Deletions",
        default: SystemJob {
          id: Some(id as i32),
          schedule: Some("@hourly".into()),
          disabled: Some(false),
        },
        callback: build_callback(move || {
          let connection_manager = connection_manager.clone();
          let object_store = object_store.clone();
          let databases = databases.clone();

          return async move {
            let _ = tokio::spawn(async move {
              let mut db_names = vec!["main".to_string()];
              db_names.extend(databases.iter().flat_map(|d| d.name.clone()));

              for db_name in db_names {
                let conn = match db_name.as_str() {
                  "main" => connection_manager.main_entry().connection,
                  name => {
                    let Ok(entry) =
                      connection_manager.get_entry(false, Some([name.to_string()].into()))
                    else {
                      continue;
                    };
                    entry.connection
                  }
                };

                if let Err(err) = delete_pending_files_job(&conn, &object_store, &db_name).await {
                  warn!("Failed to delete files: {err}");
                }
              }
            })
            .await;
            return Ok::<(), trailbase_sqlite::Error>(());
          };
        }),
      }
    }
  };
}

async fn delete_pending_files_job(
  conn: &Connection,
  object_store: &Arc<ObjectStore>,
  database_schema: &str,
) -> Result<(), FileError> {
  // TODO: Update job to delete files for all DBs.
  let rows: Vec<FileDeletionsDb> = match conn
    .write_query_values(
      format!(r#"DELETE FROM "{database_schema}"._file_deletions WHERE deleted < (UNIXEPOCH() - 900) RETURNING *"#),
      (),
    )
    .await
  {
    Ok(rows) => rows,
    Err(err) => {
      warn!("Failed to delete files: {err}");
      return Err(err.into());
    }
  };

  delete_pending_files_impl(conn, object_store, rows, database_schema).await?;

  return Ok(());
}

pub fn build_job_registry_from_config(
  config: &Config,
  data_dir: &DataDir,
  connection_manager: &ConnectionManager,
  logs_conn: &Connection,
  object_store: Arc<ObjectStore>,
) -> Result<JobRegistry, CallbackError> {
  let job_ids = [
    SystemJobId::Backup,
    SystemJobId::Heartbeat,
    SystemJobId::LogCleaner,
    SystemJobId::AuthCleaner,
    SystemJobId::QueryOptimizer,
    SystemJobId::FileDeletions,
  ];

  let jobs = JobRegistry::new();
  for job_id in job_ids {
    let DefaultSystemJob {
      name,
      default,
      callback,
    } = build_job(
      job_id,
      data_dir,
      config,
      connection_manager,
      logs_conn,
      object_store.clone(),
    );

    let config = config
      .jobs
      .system_jobs
      .iter()
      .find(|j| j.id == Some(job_id as i32))
      .unwrap_or(&default);

    let schedule = config
      .schedule
      .as_ref()
      .unwrap_or_else(|| default.schedule.as_ref().expect("startup"));

    match Schedule::from_str(schedule) {
      Ok(schedule) => match jobs.new_job(Some(job_id as i32), name, schedule, callback) {
        Some(job) => {
          if config.disabled != Some(true) {
            job.start();
          }
        }
        None => {
          error!("Duplicate job definition for '{name}'");
        }
      },
      Err(err) => {
        error!("Invalid time spec for '{name}': {err}");
      }
    };
  }

  return Ok(jobs);
}

#[cfg(test)]
mod tests {
  use super::*;
  use cron::TimeUnitSpec;

  #[test]
  fn test_cron() {
    //               sec      min   hour   day of month   month   day of week  year
    let expression = "*/100   *     *         *            *         *          *";
    assert!(Schedule::from_str(expression).is_err());

    let expression = "*/40    *     *         *            *         *          *";
    Schedule::from_str(expression).unwrap();

    let expression = "   1    2     3         4            5         6";
    let schedule = Schedule::from_str(expression).unwrap();
    assert!(schedule.seconds().includes(1));
    assert!(schedule.minutes().includes(2));
    assert!(schedule.hours().includes(3));
    assert!(schedule.days_of_month().includes(4));
    assert!(schedule.months().includes(5));
    assert!(schedule.days_of_week().includes(6));
    assert!(schedule.years().includes(1984));

    let expression = "*/40    *     *         *            *";
    assert!(Schedule::from_str(expression).is_err());
  }

  #[tokio::test]
  async fn test_scheduler() {
    // NOTE: Cron is time and not interval based, i.e. something like every 100s is not
    // representable in a single cron spec. Make sure that our cron parser detects that proerly,
    // e.g. croner does not and will just produce buggy intervals.
    // NOTE: Interval-based scheduling is generally problematic because it drifts. For something
    // like a backup you certainly want to control when and not how often it happens (e.g. at
    // night).
    let registry = JobRegistry::new();

    let (sender, receiver) = async_channel::unbounded::<()>();

    //               sec  min   hour   day of month   month   day of week  year
    let expression = "*    *     *         *            *         *         *";
    let job = registry
      .new_job(
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
      )
      .unwrap();

    job.start();

    receiver.recv().await.unwrap();

    let jobs = registry.jobs.lock();
    let first = jobs.keys().next().unwrap();

    let latest = jobs.get(first).unwrap().latest();
    let (_start_time, _duration, err_string) = latest.unwrap();
    assert_eq!(err_string, Some("result".to_string()));
  }

  #[tokio::test]
  async fn test_delete_pending_files_job() {
    let state = crate::app_state::test_state(None).await.unwrap();

    delete_pending_files_job(state.conn(), state.objectstore(), "main")
      .await
      .unwrap();
  }
}
