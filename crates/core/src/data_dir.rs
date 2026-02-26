use log::*;
use std::path::PathBuf;
use tokio::{fs, io::AsyncWriteExt};

/// The base data directory where the sqlite database, config, etc. will be stored.
#[derive(Debug, Clone)]
pub struct DataDir(pub PathBuf);

impl Default for DataDir {
  fn default() -> Self {
    Self(format!("./{}/", Self::DEFAULT).into())
  }
}

impl DataDir {
  pub const DEFAULT: &str = "traildepot";

  pub fn root(&self) -> &PathBuf {
    return &self.0;
  }

  pub fn main_db_path(&self) -> PathBuf {
    return self.data_path().join("main.db");
  }

  pub fn auth_db_path(&self) -> PathBuf {
    return self.data_path().join("auth.db");
  }

  pub fn logs_db_path(&self) -> PathBuf {
    return self.data_path().join("logs.db");
  }

  pub fn queue_db_path(&self) -> PathBuf {
    return self.data_path().join("queue.db");
  }

  pub fn data_path(&self) -> PathBuf {
    return self.0.join("data/");
  }

  pub fn config_path(&self) -> PathBuf {
    return self.0.clone();
  }

  pub fn secrets_path(&self) -> PathBuf {
    return self.0.join("secrets/");
  }

  pub fn backup_path(&self) -> PathBuf {
    return self.0.join("backups/");
  }

  pub fn migrations_path(&self) -> PathBuf {
    return self.0.join("migrations/");
  }

  pub fn uploads_path(&self) -> PathBuf {
    return self.0.join("uploads/");
  }

  pub fn key_path(&self) -> PathBuf {
    return self.secrets_path().join("keys/");
  }

  pub(crate) async fn ensure_directory_structure(&self) -> std::io::Result<()> {
    let directories = [
      self.data_path(),
      self.config_path(),
      self.backup_path(),
      self.migrations_path().join("main"),
      self.uploads_path(),
      self.key_path(),
      self.root().join("wasm/"),
    ];

    // First create directory structure.
    let mut initialized = false;
    for dir in directories {
      if !fs::try_exists(&dir).await.unwrap_or(false) {
        initialized = true;
        fs::create_dir_all(dir).await?;
      }
    }

    // Create .gitignore file but do not override
    let gitignore_path = self.root().join(".gitignore");
    if !fs::try_exists(&gitignore_path).await.unwrap_or(false) {
      initialized = true;
      let mut gitignore = fs::File::create_new(&gitignore_path).await?;
      gitignore.write_all(GIT_IGNORE.as_bytes()).await?;
    }

    if initialized {
      info!("Initialized or repaired depot: {:?}", self.root());
    }

    Ok(())
  }
}

const GIT_IGNORE: &str = r#"# Deployment-specific directories:
backups/
data/
secrets/
uploads/
wasm/
scripts/

metadata.textproto

# Runtime files, will be overriden by `trail`.
trailbase.d.ts
trailbase.js

# Any potential MaxMind GeoIP dbs.
*.mmdb
"#;
