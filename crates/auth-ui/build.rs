#![allow(clippy::needless_return)]

fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();

  let base = std::path::PathBuf::from(".");

  {
    let path = base.join("ui");
    trailbase_build::rerun_if_changed(path.join("src").join("components"));
    trailbase_build::rerun_if_changed(path.join("src").join("lib"));
    trailbase_build::rerun_if_changed(path.join("src").join("pages"));
    trailbase_build::rerun_if_changed(path.join("src").join("layouts"));

    trailbase_build::build_js(path)?;
  }

  return Ok(());
}
