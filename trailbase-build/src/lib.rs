#![allow(clippy::needless_return)]

use log::*;
use std::env;
use std::fs::{self};
use std::io::{Result, Write};
use std::path::{Path, PathBuf};

pub fn build_protos(proto_path: impl AsRef<Path>) -> Result<()> {
  let path = proto_path.as_ref().to_string_lossy();
  println!("cargo::rerun-if-changed={path}");

  let prost_config = {
    let mut config = prost_build::Config::new();
    config.enum_attribute(".", "#[derive(serde::Serialize, serde::Deserialize)]");
    config
  };

  let proto_files = vec![
    PathBuf::from(format!("{path}/config.proto")),
    PathBuf::from(format!("{path}/config_api.proto")),
    PathBuf::from(format!("{path}/vault.proto")),
  ];

  prost_reflect_build::Builder::new()
    .descriptor_pool("crate::DESCRIPTOR_POOL")
    .compile_protos_with_config(prost_config, &proto_files, &[proto_path])?;

  return Ok(());
}

pub fn copy_dir(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
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

fn write_output(mut sink: impl Write, source: &[u8], header: &str) -> Result<()> {
  sink.write_all(header.as_bytes())?;
  sink.write_all(b"\n")?;
  sink.write_all(source)?;
  sink.write_all(b"\n\n")?;
  return Ok(());
}

pub fn pnpm_run(args: &[&str]) -> Result<std::process::Output> {
  let cmd = "pnpm";
  let output = std::process::Command::new(cmd)
    .args(args)
    .output()
    .map_err(|err| {
      eprintln!("Error: Failed to run '{cmd} {}'", args.join(" "));
      return err;
    })?;

  let header = format!(
    "== {cmd} {} (cwd: {:?}) ==",
    args.join(" "),
    std::env::current_dir()?
  );
  write_output(std::io::stdout(), &output.stdout, &header)?;
  write_output(std::io::stderr(), &output.stderr, &header)?;

  if !output.status.success() {
    let msg = format!(
      "Failed to run '{args:?}'\n\t{}",
      String::from_utf8_lossy(&output.stderr)
    );

    fn is_true(v: &str) -> bool {
      return matches!(v.to_lowercase().as_str(), "true" | "1" | "");
    }

    // NOTE: We don't want to break backend-builds on frontend errors, at least for dev builds.
    match env::var("SKIP_ERROR") {
      Ok(v) if is_true(&v) => warn!("{}", msg),
      _ => {
        return Err(std::io::Error::new(std::io::ErrorKind::Other, msg));
      }
    }
  }

  Ok(output)
}

pub fn build_js(path: impl AsRef<Path>) -> Result<()> {
  let path = path.as_ref().to_string_lossy().to_string();
  // We deliberately choose not to use "--frozen-lockfile" here, since this is not a CI use-case.
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let _install_output = if out_dir.contains("target/package") {
    pnpm_run(&["--dir", &path, "install", "--ignore-workspace"])?
  } else {
    pnpm_run(&["--dir", &path, "install"])?
  };

  let _build_output = pnpm_run(&["--dir", &path, "build"])?;

  return Ok(());
}

pub fn rerun_if_changed(path: impl AsRef<Path>) {
  let path_str = path.as_ref().to_string_lossy().to_string();
  // WARN: watching non-existent paths will also trigger rebuilds.
  if !std::fs::exists(path).unwrap_or(false) {
    panic!("Path '{path_str}' doesn't exist");
  }
  println!("cargo::rerun-if-changed={path_str}");
}

pub fn init_env_logger() {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
}
