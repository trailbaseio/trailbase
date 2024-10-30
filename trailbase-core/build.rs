#![allow(clippy::needless_return)]

use log::*;
use std::env;
use std::fs::{self};
use std::io::{Result, Write};
use std::path::{Path, PathBuf};

#[allow(unused)]
fn copy_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
  fs::create_dir_all(&dst)?;
  for entry in fs::read_dir(src)? {
    let entry = entry?;
    if entry.file_name().to_str().unwrap().starts_with(".") {
      continue;
    }

    if entry.file_type()?.is_dir() {
      copy_dir(entry.path(), dst.as_ref().join(entry.file_name()))?;
    } else {
      fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
    }
  }

  return Ok(());
}

fn build_ui(path: &str) -> Result<()> {
  let pnpm_run = |args: &[&str]| -> Result<std::process::Output> {
    let output = std::process::Command::new("pnpm")
      .current_dir("..")
      .args(args)
      .output()?;

    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();

    Ok(output)
  };

  let _ = pnpm_run(&["--dir", path, "install", "--frozen-lockfile"]);

  let output = pnpm_run(&["--dir", path, "build"])?;
  if !output.status.success() {
    // NOTE: We don't want to break backend-builds on frontend errors, at least for dev builds.
    if Ok("release") == env::var("PROFILE").as_deref() {
      panic!(
        "Failed to build ui '{path}': {}",
        String::from_utf8_lossy(&output.stderr)
      );
    }
    warn!(
      "Failed to build ui '{path}': {}",
      String::from_utf8_lossy(&output.stderr)
    );
  }

  return Ok(());
}

fn build_protos() -> Result<()> {
  const PROTO_PATH: &str = "../proto";
  println!("cargo::rerun-if-changed={PROTO_PATH}");

  let prost_config = {
    let mut config = prost_build::Config::new();
    config.enum_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config
  };

  // "descriptor.proto" is provided by "libprotobuf-dev" on Debian and lives in:
  //   /usr/include/google/protobuf/descriptor.proto
  let includes = vec![PathBuf::from("/usr/include"), PathBuf::from(PROTO_PATH)];
  let proto_files = vec![
    PathBuf::from(format!("{PROTO_PATH}/config.proto")),
    PathBuf::from(format!("{PROTO_PATH}/config_api.proto")),
    PathBuf::from(format!("{PROTO_PATH}/vault.proto")),
  ];

  prost_reflect_build::Builder::new()
    .descriptor_pool("crate::DESCRIPTOR_POOL")
    //.file_descriptor_set_bytes("crate::FILE_DESCRIPTOR_SET")
    .compile_protos_with_config(prost_config, &proto_files, &includes)?;

  return Ok(());
}

fn main() -> Result<()> {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  build_protos().unwrap();

  // WARN: watching non-existent paths will also trigger rebuilds.
  println!("cargo::rerun-if-changed=../client/trailbase-ts/src/");

  {
    let path = "ui/admin";
    println!("cargo::rerun-if-changed=../{path}/src/components/");
    println!("cargo::rerun-if-changed=../{path}/src/lib/");
    let _ = build_ui(path);
  }

  {
    let path = "ui/auth";
    println!("cargo::rerun-if-changed=../{path}/src/components/");
    println!("cargo::rerun-if-changed=../{path}/src/lib/");
    println!("cargo::rerun-if-changed=../{path}/src/pages/");
    println!("cargo::rerun-if-changed=../{path}/src/layouts/");
    let _ = build_ui("ui/auth");
  }

  return Ok(());
}
