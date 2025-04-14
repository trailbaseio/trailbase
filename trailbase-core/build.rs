#![allow(clippy::needless_return)]

use std::io::Result;
use std::path::PathBuf;

fn build_protos() -> Result<()> {
  const PROTO_PATH: &str = "./proto";
  println!("cargo::rerun-if-changed={PROTO_PATH}");

  let prost_config = {
    let mut config = prost_build::Config::new();
    config.enum_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config
  };

  let proto_files = vec![
    PathBuf::from(format!("{PROTO_PATH}/config.proto")),
    PathBuf::from(format!("{PROTO_PATH}/config_api.proto")),
    PathBuf::from(format!("{PROTO_PATH}/vault.proto")),
  ];

  prost_reflect_build::Builder::new()
    .descriptor_pool("crate::DESCRIPTOR_POOL")
    .compile_protos_with_config(prost_config, &proto_files, &[PathBuf::from(PROTO_PATH)])?;

  return Ok(());
}

fn main() -> Result<()> {
  rustc_tools_util::setup_version_info!();

  build_protos()?;

  return Ok(());
}
