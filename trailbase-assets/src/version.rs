use std::path::PathBuf;
use std::process::Command;
use std::str;

/// This macro creates the version string during compilation from the
/// current environment
#[macro_export]
macro_rules! get_version_info {
  () => {{
    let major = std::env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap();
    let minor = std::env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap();
    let patch = std::env!("CARGO_PKG_VERSION_PATCH").parse::<u16>().unwrap();
    let crate_name = String::from(std::env!("CARGO_PKG_NAME"));

    let host_compiler = std::option_env!("RUSTC_RELEASE_CHANNEL").map(str::to_string);
    let commit_hash = std::option_env!("GIT_HASH").map(str::to_string);
    let commit_date = std::option_env!("GIT_COMMIT_DATE").map(str::to_string);
    let version_tag = std::option_env!("GIT_VERSION_TAG").map(str::to_string);

    $crate::version::VersionInfo {
      major,
      minor,
      patch,
      host_compiler,
      commit_hash,
      commit_date,
      version_tag,
      crate_name,
    }
  }};
}

/// This macro can be used in `build.rs` to automatically set the needed environment values, namely
/// `GIT_HASH`, `GIT_COMMIT_DATE`  and `RUSTC_RELEASE_CHANNEL`
#[macro_export]
macro_rules! setup_version_info {
  () => {{
    let _ = $crate::version::rerun_if_git_changes();
    println!(
      "cargo:rustc-env=GIT_HASH={}",
      $crate::version::get_commit_hash().unwrap_or_default()
    );
    println!(
      "cargo:rustc-env=GIT_COMMIT_DATE={}",
      $crate::version::get_commit_date().unwrap_or_default()
    );
    println!(
      "cargo:rustc-env=GIT_VERSION_TAG={}",
      $crate::version::get_version_tag().unwrap_or_default()
    );
    let compiler_version = $crate::version::get_compiler_version();
    println!(
      "cargo:rustc-env=RUSTC_RELEASE_CHANNEL={}",
      $crate::version::get_channel(compiler_version)
    );
  }};
}

// some code taken and adapted from RLS and cargo
pub struct VersionInfo {
  pub major: u8,
  pub minor: u8,
  pub patch: u16,
  pub host_compiler: Option<String>,
  pub commit_hash: Option<String>,
  pub commit_date: Option<String>,
  pub version_tag: Option<String>,
  pub crate_name: String,
}

impl std::fmt::Display for VersionInfo {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let hash = self.commit_hash.clone().unwrap_or_default();
    let hash_trimmed = hash.trim();

    let date = self.commit_date.clone().unwrap_or_default();
    let date_trimmed = date.trim();

    if (hash_trimmed.len() + date_trimmed.len()) > 0 {
      write!(
        f,
        "{} {}.{}.{} ({hash_trimmed} {date_trimmed})",
        self.crate_name, self.major, self.minor, self.patch,
      )?;
    } else {
      write!(
        f,
        "{} {}.{}.{}",
        self.crate_name, self.major, self.minor, self.patch
      )?;
    }

    return Ok(());
  }
}

impl std::fmt::Debug for VersionInfo {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "VersionInfo {{ crate_name: \"{}\", major: {}, minor: {}, patch: {}",
      self.crate_name, self.major, self.minor, self.patch,
    )?;
    if let Some(ref commit_hash) = self.commit_hash {
      write!(f, ", commit_hash: \"{}\"", commit_hash.trim(),)?;
    }
    if let Some(ref commit_date) = self.commit_date {
      write!(f, ", commit_date: \"{}\"", commit_date.trim())?;
    }
    if let Some(ref host_compiler) = self.host_compiler {
      write!(f, ", host_compiler: \"{}\"", host_compiler.trim())?;
    }

    write!(f, " }}")?;

    return Ok(());
  }
}

#[must_use]
fn get_output(cmd: &str, args: &[&str]) -> Option<String> {
  let output = Command::new(cmd).args(args).output().ok()?;
  let mut stdout = output.status.success().then_some(output.stdout)?;
  // Remove trailing newlines.
  while stdout.last().copied() == Some(b'\n') {
    stdout.pop();
  }
  return String::from_utf8(stdout).ok();
}

#[must_use]
pub fn rerun_if_git_changes() -> Option<()> {
  // Make sure we get rerun when the git commit changes.
  // We want to watch two files: HEAD, which tracks which branch we are on,
  // and the file for that branch that tracks which commit is checked out.

  // First, find the `HEAD` file. This should work even with worktrees.
  let git_head_file = PathBuf::from(get_output("git", &["rev-parse", "--git-path", "HEAD"])?);
  if git_head_file.exists() {
    println!("cargo::rerun-if-changed={}", git_head_file.display());
  }

  // Determine the name of the current ref.
  // This will quit if HEAD is detached.
  let git_head_ref = get_output("git", &["symbolic-ref", "-q", "HEAD"])?;
  // Ask git where this ref is stored.
  let git_head_ref_file = PathBuf::from(get_output(
    "git",
    &["rev-parse", "--git-path", &git_head_ref],
  )?);
  // If this ref is packed, the file does not exist. However, the checked-out branch is never (?)
  // packed, so we should always be able to find this file.
  if git_head_ref_file.exists() {
    println!("cargo::rerun-if-changed={}", git_head_ref_file.display());
  }

  return Some(());
}

#[must_use]
pub fn get_commit_hash() -> Option<String> {
  let mut stdout = get_output("git", &["rev-parse", "HEAD"])?;
  stdout.truncate(10);
  return Some(stdout);
}

#[must_use]
pub fn get_commit_date() -> Option<String> {
  return get_output("git", &["log", "-1", "--date=short", "--pretty=format:%cd"]);
}

#[must_use]
pub fn get_version_tag() -> Option<String> {
  return get_output("git", &["describe", "--tags", "--match=v*"]);
}

#[must_use]
pub fn get_compiler_version() -> Option<String> {
  return get_output("rustc", &["-V"]);
}

#[must_use]
pub fn get_channel(compiler_version: Option<String>) -> String {
  if let Ok(channel) = std::env::var("CFG_RELEASE_CHANNEL") {
    return channel;
  }

  // if that failed, try to ask rustc -V, do some parsing and find out
  if let Some(rustc_output) = compiler_version {
    if rustc_output.contains("beta") {
      return String::from("beta");
    } else if rustc_output.contains("nightly") {
      return String::from("nightly");
    }
  }

  // default to stable
  return String::from("stable");
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_struct_local() {
    let vi = get_version_info!();
    assert_eq!(vi.major, 0);
    assert_eq!(vi.minor, 2);
    assert_eq!(vi.patch, 0);
    assert_eq!(vi.crate_name, "trailbase-assets");
    // hard to make positive tests for these since they will always change
    assert!(vi.commit_hash.is_none());
    assert!(vi.commit_date.is_none());

    assert!(vi.host_compiler.is_none());
  }

  #[test]
  fn test_display_local() {
    let vi = get_version_info!();
    assert_eq!(vi.to_string(), "trailbase-assets 0.2.0");
  }

  #[test]
  fn test_debug_local() {
    let vi = get_version_info!();
    let s = format!("{vi:?}");
    assert_eq!(
      s,
      "VersionInfo { crate_name: \"trailbase-assets\", major: 0, minor: 2, patch: 0 }"
    );
  }
}
