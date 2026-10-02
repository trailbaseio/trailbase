use log::*;
use std::sync::{Arc, OnceLock};
use std::thread::{JoinHandle, sleep, spawn};
use std::time::{Duration, SystemTime};

pub fn start_watchdog<T: Send + Sync + 'static>(
  resource: &Arc<T>,
  cb: impl FnOnce(&T) + Send + Sync + 'static,
  timeout: std::time::Duration,
) {
  let resource = Arc::downgrade(&resource);

  let watcher = move || {
    debug!("WATCHDOG: started");

    let started = SystemTime::now();
    loop {
      let elapsed = SystemTime::now()
        .duration_since(started)
        .unwrap_or_default();

      if elapsed >= timeout {
        error!("WATCHDOG: expired");

        if let Some(resource) = resource.upgrade() {
          // Spawn callback in its own thread to still eventually terminate even if the callback
          // blocks, e.g. `db.close()` hangs.
          spawn(move || cb(&resource));

          // The the callback some time to run.
          sleep(Duration::from_secs(60));
        } else {
          info!("WATCHDOG: resource already consumed");
        }

        error!("WATCHDOG: terminating process");
        std::process::exit(12);
      }

      sleep(Duration::from_mins(1));
    }
  };

  static WATCHDOG_THREAD: OnceLock<JoinHandle<()>> = OnceLock::new();
  WATCHDOG_THREAD.get_or_init(|| spawn(watcher));
}
