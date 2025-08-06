#![allow(clippy::needless_return)]

use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
  trailbase_build::init_env_logger();

  let path = PathBuf::from("assets").join("runtime");
  trailbase_build::rerun_if_changed(path.join("src"));

  trailbase_build::build_js(path)?;

  return Ok(());
}
