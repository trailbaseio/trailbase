use std::os::unix::process::CommandExt;
use std::time::{Duration, SystemTime};

pub struct Server {
  child: Option<std::process::Child>,
}

impl Drop for Server {
  fn drop(&mut self) {
    if let Some(mut child) = std::mem::take(&mut self.child) {
      child.kill().unwrap();
    }
  }
}

fn port() -> u16 {
  const DEFAULT_PORT: u16 = 4057;
  if let Ok(port) = std::env::var("PORT") {
    return port.parse().unwrap_or(DEFAULT_PORT);
  }
  return DEFAULT_PORT;
}

pub fn site() -> String {
  return format!("http://127.0.0.1:{}", port());
}

pub async fn start_server(timeout: Duration) -> Result<Option<Server>, std::io::Error> {
  let mut child = if port() == 4000 {
    // Use an externally bootstrapped server.
    None
  } else {
    let cwd = std::env::current_dir()?;
    assert!(cwd.ends_with("client"));

    let command_cwd = cwd.parent().unwrap().parent().unwrap();

    log::info!("Building dev server... (cold builds may take a while)");
    let _output = std::process::Command::new("python3")
      .args(&[
        "client/runner.py",
        "build",
        #[cfg(feature = "ws")]
        {
          "--ws"
        },
      ])
      .current_dir(&command_cwd)
      .output()?;

    log::info!("Starting the dev server...");
    let mut run_command = std::process::Command::new("python3");
    let args = [
      "client/runner.py".to_string(),
      "run".to_string(),
      #[cfg(feature = "ws")]
      {
        "--ws".to_string()
      },
      format!("--port={}", port()),
      "--runtime-threads=2".to_string(),
    ];

    #[cfg(target_os = "linux")]
    unsafe {
      run_command.pre_exec(|| {
        use rustix::process::{Resource, Rlimit, getrlimit, setrlimit};

        let current_limits = getrlimit(Resource::Nofile);
        eprintln!("Current process limits: {current_limits:?}");

        if let Err(err) = setrlimit(
          Resource::Nofile,
          Rlimit {
            // Soft limit.
            current: Some(current_limits.maximum.unwrap_or(1024).min(2048)),
            // Hard limit. Don't use None, which implies infinite.
            maximum: current_limits.maximum,
          },
        ) {
          eprintln!("ERROR: Failed to raise OPEN FILE LIMIT: {err}");
        }

        return Ok(());
      });
    }

    Some(run_command.args(&args).current_dir(&command_cwd).spawn()?)
  };

  let client = reqwest::Client::new();
  let url = format!("{site}/api/healthcheck", site = site());

  let started_waiting = SystemTime::now();
  loop {
    if SystemTime::now().duration_since(started_waiting).unwrap() >= timeout {
      panic!("Server did not get healthy");
    }

    if let Some(child) = &mut child
      && let Ok(Some(status)) = child.try_wait()
    {
      panic!("Test server already exited with {status}. Maybe other server running at same port?");
    };

    if let Ok(response) = client.get(&url).send().await {
      if let Ok(body) = response.text().await {
        if body.to_uppercase() == "OK" {
          println!("Server found healthy @{}", site());

          return Ok(Some(Server { child }));
        }
      }
    }

    const INTERVAL: Duration = Duration::from_millis(500);
    tokio::time::sleep(INTERVAL).await;
  }
}
