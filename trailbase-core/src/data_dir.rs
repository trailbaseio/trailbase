use std::path::PathBuf;
use tokio::{fs, io::AsyncWriteExt};
use tracing::*;

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

  pub fn logs_db_path(&self) -> PathBuf {
    return self.data_path().join("logs.db");
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

  fn directories(&self) -> Vec<PathBuf> {
    return vec![
      self.data_path(),
      self.config_path(),
      self.backup_path(),
      self.migrations_path(),
      self.uploads_path(),
      self.key_path(),
    ];
  }

  pub(crate) async fn ensure_directory_structure(&self) -> std::io::Result<()> {
    // First create directory structure.
    let root = self.root();
    if !fs::try_exists(root).await.unwrap_or(false) {
      fs::create_dir_all(root).await?;

      // Create .gitignore file.
      let mut gitignore = fs::File::create_new(root.join(".gitignore")).await?;
      gitignore.write_all(GIT_IGNORE.as_bytes()).await?;

      info!("Initialized fresh data dir: {:?}", root);
    }

    for dir in self.directories() {
      if !fs::try_exists(&dir).await.unwrap_or(false) {
        fs::create_dir_all(dir).await?;
      }
    }

    Ok(())
  }
}

const GIT_IGNORE: &str = r#"
backups/
data/
secrets/
uploads/
"#;
