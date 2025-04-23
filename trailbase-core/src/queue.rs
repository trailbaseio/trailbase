use thiserror::Error;

use crate::data_dir::DataDir;

#[derive(Debug, Error)]
pub enum QueueError {
  #[error("SQLite ext error: {0}")]
  SqliteExtension(#[from] trailbase_extension::Error),
  #[error("SQLite error: {0}")]
  Sqlite(#[from] trailbase_sqlite::Error),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
}

#[cfg(feature = "queue")]
pub(crate) mod queue_impl {
  use log::*;
  use serde::{Deserialize, Serialize};

  use super::QueueError;

  #[derive(Debug, Serialize, Deserialize)]
  pub enum Job {
    Something(),
  }

  pub async fn handle_job(job: Job) -> Result<(), QueueError> {
    match job {
      Job::Something() => {
        info!("Queue got something");
      }
    }

    return Ok(());
  }

  #[cfg(feature = "queue")]
  pub(crate) type QueueStorage = trailbase_apalis::sqlite::SqliteStorage<Job>;
}

#[derive(Clone)]
pub struct Queue {
  #[cfg(feature = "queue")]
  pub(crate) storage: queue_impl::QueueStorage,
}

impl Queue {
  #[allow(unused)]
  pub(crate) async fn new(data_dir: Option<&DataDir>) -> Result<Self, QueueError> {
    return Ok(Self {
      #[cfg(feature = "queue")]
      storage: init_queue_storage(data_dir).await?,
    });
  }

  #[cfg(feature = "queue")]
  #[allow(unused)]
  pub(crate) async fn run(&self) -> Result<(), QueueError> {
    use apalis::prelude::*;

    let monitor = Monitor::new().register({
      WorkerBuilder::new("default-worker")
        // .enable_tracing()
        .backend(self.storage.clone())
        .build_fn(queue_impl::handle_job)
    });

    return Ok(monitor.run().await?);
  }
}

#[cfg(feature = "queue")]
pub(crate) async fn init_queue_storage(
  data_dir: Option<&DataDir>,
) -> Result<queue_impl::QueueStorage, QueueError> {
  let queue_path = data_dir.map(|d| d.queue_db_path());
  let conn = trailbase_sqlite::Connection::new(
    || -> Result<_, trailbase_sqlite::Error> {
      return Ok(
        crate::connection::connect_rusqlite_without_default_extensions_and_schemas(
          queue_path.clone(),
        )?,
      );
    },
    None,
  )?;

  trailbase_apalis::sqlite::SqliteStorage::setup(&conn).await?;

  let config = trailbase_apalis::Config::new("ns::trailbase");
  return Ok(queue_impl::QueueStorage::new_with_config(conn, config));
}

#[cfg(test)]
#[cfg(feature = "queue")]
mod tests {
  use super::queue_impl::*;
  use super::*;

  use apalis::prelude::*;

  #[tokio::test]
  async fn test_queue() {
    let mut queue = Queue::new(None).await.unwrap();

    let (sender, receiver) = async_channel::unbounded::<()>();

    let storage = queue.storage.clone();
    let _ = tokio::spawn(async move {
      let monitor = Monitor::new().register({
        WorkerBuilder::new("default-worker")
          .data(sender)
          .backend(storage)
          .build_fn(
            async |job: Job, sender: Data<async_channel::Sender<()>>| -> Result<(), QueueError> {
              match job {
                Job::Something() => sender.send(()).await.unwrap(),
              }

              return Ok(());
            },
          )
      });

      return monitor.run().await;
    });

    let job = queue.storage.push(Job::Something()).await.unwrap();

    let entry = queue.storage.fetch_by_id(&job.task_id).await.unwrap();
    assert!(entry.is_some());

    receiver.recv().await.unwrap();
  }
}
