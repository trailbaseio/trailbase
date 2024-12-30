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
    if entry.file_name().to_string_lossy().starts_with(".") {
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

fn build_js(path: &str) -> Result<()> {
  let pnpm_run = |args: &[&str]| -> Result<std::process::Output> {
    let cmd = "pnpm";
    let output = std::process::Command::new(cmd)
      .args(args)
      .output()
      .map_err(|err| {
        eprintln!("Error: Failed to run '{cmd} {}'", args.join(" "));
        return err;
      })?;

    std::io::stdout().write_all(&output.stdout)?;
    std::io::stderr().write_all(&output.stderr)?;

    Ok(output)
  };

  // We deliberately chose not use "--frozen-lockfile" here, since this is not a CI use-case.
  let _install_output = pnpm_run(&["--dir", path, "install"])?;

  let build_output = pnpm_run(&["--dir", path, "build"])?;
  if !build_output.status.success() {
    // NOTE: We don't want to break backend-builds on frontend errors, at least for dev builds.
    if env::var("SKIP_ERROR").is_err() {
      panic!(
        "Failed to build js '{path}': {}",
        String::from_utf8_lossy(&build_output.stderr)
      );
    }
    warn!(
      "Failed to build js '{path}': {}",
      String::from_utf8_lossy(&build_output.stderr)
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
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  build_protos()?;

  // WARN: watching non-existent paths will also trigger rebuilds.
  println!("cargo::rerun-if-changed=../client/trailbase-ts/src/");

  {
    let path = "ui/admin";
    println!("cargo::rerun-if-changed={path}/src/components/");
    println!("cargo::rerun-if-changed={path}/src/lib/");
    build_js(path)?;
  }

  {
    let path = "ui/auth";
    println!("cargo::rerun-if-changed={path}/src/components/");
    println!("cargo::rerun-if-changed={path}/src/lib/");
    println!("cargo::rerun-if-changed={path}/src/pages/");
    println!("cargo::rerun-if-changed={path}/src/layouts/");
    build_js(path)?;
  }

  {
    println!("cargo::rerun-if-changed=js/src/");
    build_js("js")?;
  }

  return Ok(());
}
