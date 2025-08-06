#![allow(clippy::needless_return)]

fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();
  trailbase_build::setup_version_info!();

  return Ok(());
}
