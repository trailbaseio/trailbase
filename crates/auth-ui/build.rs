#![allow(clippy::needless_return)]

fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();
  trailbase_build::setup_version_info!();

  let base = std::path::PathBuf::from(".");

  // place TS-TS bindings locally
  let bindings_path = "ui/bindings";
  let _ = std::fs::create_dir_all(base.join(bindings_path));
  println!("cargo:rustc-env=TS_RS_EXPORT_DIR=./{bindings_path}");

  // Build UI
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
