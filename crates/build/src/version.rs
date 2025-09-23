use std::path::PathBuf;
use std::process::Command;
use std::str;

/// This macro creates the version string during compilation from the
/// current environment
#[macro_export]
macro_rules! get_version_info {
  () => {{
    use std::{env, option_env};

    let crate_name = env!("CARGO_PKG_NAME").to_string();
    let major = env!("CARGO_PKG_VERSION_MAJOR").parse::<u8>().unwrap();
    let minor = env!("CARGO_PKG_VERSION_MINOR").parse::<u8>().unwrap();
    let patch = env!("CARGO_PKG_VERSION_PATCH").parse::<u16>().unwrap();

    let host_compiler = option_env!("RUSTC_RELEASE_CHANNEL").map(str::to_string);

    let git_commit_hash = option_env!("GIT_HASH").map(str::to_string);
    let git_commit_date = option_env!("GIT_COMMIT_DATE").map(str::to_string);
    let git_version_tag = option_env!("GIT_VERSION_TAG").map(str::to_string);

    $crate::version::VersionInfo {
      crate_name,
      crate_version: $crate::version::CrateVersion {
        major,
        minor,
        patch,
      },
      host_compiler,
      git_commit_hash,
      git_commit_date,
      git_version_tag,
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

#[derive(Clone, Debug, Default)]
pub struct CrateVersion {
  pub major: u8,
  pub minor: u8,
  pub patch: u16,
}

#[derive(Clone, Debug)]
pub struct GitVersion {
  pub major: u8,
  pub minor: u8,
  pub patch: u16,

  pub commits_since: Option<u16>,
}

#[derive(Clone, Debug, Default)]
pub struct VersionInfo {
  /// Name of the crate as defined in its Cargo.toml.
  pub crate_name: String,
  /// Version as defined by the crate. This is different from the git version.
  pub crate_version: CrateVersion,

  /// Build metadata.
  pub host_compiler: Option<String>,

  /// Git metadata.

  /// Full git commit hash.
  pub git_commit_hash: Option<String>,
  /// Pretty-printed git commit date.
  pub git_commit_date: Option<String>,

  /// Git description of latest "version tag", i.e. vX.Y.Z. Format:
  ///   `vX.Y.Z-<#commits since>-<commit hash>`.
  pub git_version_tag: Option<String>,
}

impl VersionInfo {
  pub fn git_version(&self) -> Option<GitVersion> {
    let version_tag = self.git_version_tag.as_ref()?;

    let re =
      regex::Regex::new(r#"v(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)-(?P<since>[0-9a-z]+)"#)
        .unwrap();

    let cap = re.captures(version_tag)?;
    return Some(GitVersion {
      major: cap["major"].parse().ok()?,
      minor: cap["minor"].parse().ok()?,
      patch: cap["patch"].parse().ok()?,
      commits_since: cap["since"].parse().ok(),
    });
  }
}

impl std::fmt::Display for VersionInfo {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let v = &self.crate_version;

    match (&self.git_commit_hash, &self.git_commit_date) {
      (Some(hash), Some(date)) => {
        write!(
          f,
          "{} {}.{}.{} ({} {})",
          self.crate_name,
          v.major,
          v.minor,
          v.patch,
          hash.trim(),
          date.trim()
        )?;
      }
      _ => {
        write!(f, "{} {}.{}.{}", self.crate_name, v.major, v.minor, v.patch)?;
      }
    }

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
  return get_output("git", &["rev-parse", "HEAD"]);
}

#[must_use]
pub fn get_commit_date() -> Option<String> {
  return get_output("git", &["log", "-1", "--date=short", "--pretty=format:%cd"]);
}

#[must_use]
pub fn get_version_tag() -> Option<String> {
  return get_output("git", &["describe", "--tags", "--match=v*", "--long"]);
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
    assert_eq!(vi.crate_version.major, 0);
    assert!(vi.crate_version.minor >= 1);
    assert_eq!(vi.crate_name, "trailbase-build");
    // hard to make positive tests for these since they will always change
    assert!(vi.git_commit_hash.is_none());
    assert!(vi.git_commit_date.is_none());

    assert!(vi.host_compiler.is_none());
  }

  #[test]
  fn test_display_local() {
    let vi = get_version_info!();
    let re = regex::Regex::new("trailbase-build 0.[0-9]+.[0-9]+").unwrap();
    assert!(re.is_match(&vi.to_string()));
  }

  #[test]
  fn test_git_version() {
    let vi = VersionInfo {
      git_version_tag: Some("v0.17.3-9-g5d422ec7".to_string()),
      ..Default::default()
    };
    let git = vi.git_version().unwrap();

    assert!(git.commits_since.is_some())
  }
}
