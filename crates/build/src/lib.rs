#![allow(clippy::needless_return)]

pub mod version;

use log::*;
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
    PathBuf::from(format!("{path}/metadata.proto")),
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
    return Err(std::io::Error::other(format!(
      "Failed to run '{args:?}'\n\t{}",
      String::from_utf8_lossy(&output.stderr)
    )));
  }

  Ok(output)
}

pub fn build_js(path: impl AsRef<Path>) -> Result<()> {
  let path = path.as_ref().to_string_lossy().to_string();
  let strict_offline: bool = matches!(
    std::env::var("PNPM_OFFLINE").as_deref(),
    Ok("TRUE") | Ok("true") | Ok("1")
  );

  // Note that `--frozen-lockfile` and `-ignore-workspace` are practically exclusive
  // Because ignoring the workspace one, will require to create a new lockfile.
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let build_result = if out_dir.contains("target/package") {
    // When we build cargo packages, we cannot rely on the workspace itself and prior installs.
    pnpm_run(&["--dir", &path, "install", "--ignore-workspace"])
  } else {
    // `trailbase-assets` and `trailbase-js` both build JS packages. We've seen issues with
    // parallel installs in the past. Our current approach is to recommend installing workspace
    // JS deps upfront in combination with `--prefer-offline`. We used to use plain `--offline`,
    // however this adds an extra mandatory step when vendoring trailbase for framework use-cases.
    let args = if strict_offline {
      ["--dir", &path, "install", "--frozen-lockfile", "--offline"]
    } else {
      [
        "--dir",
        &path,
        "install",
        "--prefer-frozen-lockfile",
        "--prefer-offline",
      ]
    };
    let build_result = pnpm_run(&args);
    if build_result.is_err() {
      error!(
        "`pnpm {}` failed. Make sure to install all JS deps first using `pnpm install`",
        args.join(" ")
      )
    }
    build_result
  };

  let _ = build_result?;

  let build_output = pnpm_run(&["--dir", &path, "build"]);
  if build_output.is_err() && cfg!(windows) {
    error!(
      "pnpm build failed on Windows. Make sure to enable symlinks: `git config core.symlinks true && git reset --hard`."
    );
  }

  let _ = build_output?;

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
