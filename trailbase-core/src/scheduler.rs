use chrono::{Duration, Utc};
use log::*;
use std::future::Future;
use tokio_rusqlite::params;

use crate::app_state::AppState;
use crate::constants::{DEFAULT_REFRESH_TOKEN_TTL, LOGS_RETENTION_DEFAULT, SESSION_TABLE};

#[derive(Default)]
pub struct AbortOnDrop {
  handles: Vec<tokio::task::AbortHandle>,
}

impl AbortOnDrop {
  fn add_periodic_task<F, Fut>(&mut self, period: Duration, f: F)
  where
    F: 'static + Sync + Send + Fn() -> Fut,
    Fut: Sync + Send + Future,
  {
    let handle = tokio::spawn(async move {
      let period = period.to_std().unwrap();
      let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
      loop {
        interval.tick().await;
        f().await;
      }
    });

    self.handles.push(handle.abort_handle());
  }
}

impl Drop for AbortOnDrop {
  fn drop(&mut self) {
    for h in &self.handles {
      h.abort();
    }
  }
}

pub(super) fn start_periodic_tasks(app_state: &AppState) -> AbortOnDrop {
  let mut tasks = AbortOnDrop::default();

  tasks.add_periodic_task(Duration::seconds(60), || async {
    info!("alive");
  });

  // Backup job.
  let conn = app_state.conn().clone();
  let backup_file = app_state.data_dir().backup_path().join("backup.db");
  let backup_interval = app_state
    .access_config(|c| c.server.backup_interval_sec)
    .map_or(Duration::zero(), Duration::seconds);
  if !backup_interval.is_zero() {
    tasks.add_periodic_task(backup_interval, move || {
      let conn = conn.clone();
      let backup_file = backup_file.clone();

      async move {
        let result = conn
          .call(|conn| {
            return Ok(conn.backup(
              rusqlite::DatabaseName::Main,
              backup_file,
              /* progress= */ None,
            )?);
          })
          .await;

        match result {
          Ok(_) => info!("Backup complete"),
          Err(err) => error!("Backup failed: {err}"),
        };
      }
    });
  }

  // Logs cleaner.
  let logs_conn = app_state.logs_conn().clone();
  let retention = app_state
    .access_config(|c| c.server.logs_retention_sec)
    .map_or(LOGS_RETENTION_DEFAULT, Duration::seconds);

  if !retention.is_zero() {
    tasks.add_periodic_task(Duration::hours(2), move || {
      let logs_conn = logs_conn.clone();

      tokio::spawn(async move {
        let timestamp = (Utc::now() - retention).timestamp();
        match logs_conn
          .execute("DELETE FROM _logs WHERE created < $1", params!(timestamp))
          .await
        {
          Ok(_) => info!("Successfully pruned logs"),
          Err(err) => warn!("Failed to clean up old logs: {err}"),
        };
      })
    });
  }

  // Refresh token cleaner.
  let state = app_state.clone();
  tasks.add_periodic_task(Duration::hours(12), move || {
    let state = state.clone();

    tokio::spawn(async move {
      let refresh_token_ttl = state
        .access_config(|c| c.auth.refresh_token_ttl_sec)
        .map_or(DEFAULT_REFRESH_TOKEN_TTL, Duration::seconds);

      let timestamp = (Utc::now() - refresh_token_ttl).timestamp();

      match state
        .user_conn()
        .execute(
          &format!("DELETE FROM '{SESSION_TABLE}' WHERE updated < $1"),
          params!(timestamp),
        )
        .await
      {
        Ok(count) => info!("Successfully pruned {count} old sessions."),
        Err(err) => warn!("Failed to clean up sessions: {err}"),
      };
    })
  });

  // Optimizer
  let conn = app_state.conn().clone();
  tasks.add_periodic_task(Duration::hours(24), move || {
    let conn = conn.clone();

    tokio::spawn(async move {
      match conn.execute("PRAGMA optimize", ()).await {
        Ok(_) => info!("Successfully ran query optimizer"),
        Err(err) => warn!("query optimizer failed: {err}"),
      };
    })
  });

  return tasks;
}
