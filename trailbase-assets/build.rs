#![allow(clippy::needless_return)]

use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
  trailbase_build::init_env_logger();

  // WARN: watching non-existent paths will also trigger rebuilds.
  trailbase_build::rerun_if_changed("js/client/src/");

  {
    let path = PathBuf::from("js/admin");
    trailbase_build::rerun_if_changed(path.join("src/components/"));
    trailbase_build::rerun_if_changed(path.join("src/lib/"));

    trailbase_build::build_js(path)?;
  }

  {
    let path = PathBuf::from("js/auth");
    trailbase_build::rerun_if_changed(path.join("src/components/"));
    trailbase_build::rerun_if_changed(path.join("src/lib/"));
    trailbase_build::rerun_if_changed(path.join("src/pages/"));
    trailbase_build::rerun_if_changed(path.join("src/layouts/"));

    trailbase_build::build_js(path)?;
  }

  return Ok(());
}
