#![allow(clippy::needless_return)]

use log::*;
use std::env;
use std::fs::{self};
use std::io::{Result, Write};
use std::path::Path;

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

fn write_output(mut sink: impl Write, source: &[u8], header: &str) -> Result<()> {
  sink.write_all(header.as_bytes())?;
  sink.write_all(b"\n")?;
  sink.write_all(source)?;
  sink.write_all(b"\n\n")?;
  return Ok(());
}

fn pnpm_run(args: &[&str]) -> Result<std::process::Output> {
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

fn build_js(path: &str) -> Result<()> {
  // We deliberately chose not to use "--frozen-lockfile" here, since this is not a CI use-case.
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let _install_output = if out_dir.contains("target/package") {
    pnpm_run(&["--dir", path, "install", "--ignore-workspace"])?
  } else {
    pnpm_run(&["--dir", path, "install"])?
  };

  let _build_output = pnpm_run(&["--dir", path, "build"])?;

  return Ok(());
}

fn main() -> Result<()> {
  env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

  // WARN: watching non-existent paths will also trigger rebuilds.
  println!("cargo::rerun-if-changed=js/client/src/");

  {
    let path = "js/admin";
    println!("cargo::rerun-if-changed={path}/src/components/");
    println!("cargo::rerun-if-changed={path}/src/lib/");
    build_js(path)?;
  }

  {
    let path = "js/auth";
    println!("cargo::rerun-if-changed={path}/src/components/");
    println!("cargo::rerun-if-changed={path}/src/lib/");
    println!("cargo::rerun-if-changed={path}/src/pages/");
    println!("cargo::rerun-if-changed={path}/src/layouts/");
    build_js(path)?;
  }

  {
    println!("cargo::rerun-if-changed=js/runtime/src/");
    build_js("js/runtime")?;
  }

  return Ok(());
}
