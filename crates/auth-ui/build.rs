#![allow(clippy::needless_return)]

use std::{io::Result, path::PathBuf};

fn main() -> Result<()> {
  trailbase_build::init_env_logger();

  // Rely on trailbase-asset dep for now to build.
  //
  // let base = PathBuf::from("../assets/js/");
  //
  // {
  //   let path = base.join("auth");
  //   trailbase_build::rerun_if_changed(path.join("src").join("components"));
  //   trailbase_build::rerun_if_changed(path.join("src").join("lib"));
  //   trailbase_build::rerun_if_changed(path.join("src").join("pages"));
  //   trailbase_build::rerun_if_changed(path.join("src").join("layouts"));
  //
  //   trailbase_build::build_js(path)?;
  // }

  return Ok(());
}
