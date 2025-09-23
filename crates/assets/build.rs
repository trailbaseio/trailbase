#![allow(clippy::needless_return)]

use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
  trailbase_build::init_env_logger();

  let base = PathBuf::from("js");

  // NOTE: Client isn't separately build and packed, it's merely a dependency of admin & auth that
  // we watch for changes.
  trailbase_build::rerun_if_changed(base.join("client").join("src"));

  {
    let path = base.join("admin");
    trailbase_build::rerun_if_changed(path.join("src").join("components"));
    trailbase_build::rerun_if_changed(path.join("src").join("lib"));

    trailbase_build::build_js(path)?;
  }

  return Ok(());
}
