#![allow(clippy::needless_return)]

fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();

  println!("cargo::rerun-if-changed=migrations");

  trailbase_assets::setup_version_info!();

  trailbase_build::build_protos("./proto")?;

  return Ok(());
}
