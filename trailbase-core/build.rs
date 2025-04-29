#![allow(clippy::needless_return)]

fn main() -> std::io::Result<()> {
  trailbase_build::init_env_logger();

  rustc_tools_util::setup_version_info!();

  trailbase_build::build_protos("./proto")?;

  return Ok(());
}
