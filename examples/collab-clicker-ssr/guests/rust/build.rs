#![allow(clippy::needless_return)]

#[cfg(not(feature = "bundle"))]
fn main() -> std::io::Result<()> {
  return Ok(());
}

#[cfg(feature = "bundle")]
fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();

  let path = std::path::PathBuf::from("../..");
  trailbase_build::rerun_if_changed(path.join("src"));
  trailbase_build::build_js(path)?;

  return Ok(());
}
