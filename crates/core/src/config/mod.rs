pub mod error;
pub mod proto;
pub mod textproto;
pub(crate) mod validate;
pub(crate) mod vault;

use log::*;
use std::fs;
use std::path::PathBuf;

use crate::config::vault::{
  load_vault_textproto_or_default, merge_vault_and_env, redact_secrets, vault_path,
};
use crate::connection::ConnectionManager;
use crate::data_dir::DataDir;

pub use crate::config::error::ConfigError;
pub use crate::config::textproto::Textproto;
pub use crate::config::validate::validate_config;

pub fn config_path(data_dir: &DataDir) -> PathBuf {
  const CONFIG_FILENAME: &str = "config.textproto";
  return data_dir.config_path().join(CONFIG_FILENAME);
}

pub fn maybe_load_config_textproto_unverified(
  data_dir: &DataDir,
) -> Result<Option<proto::Config>, ConfigError> {
  return match fs::read_to_string(config_path(data_dir)) {
    Ok(contents) => Ok(Some(proto::Config::from_text(&contents)?)),
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(err) => Err(err.into()),
  };
}

/// Load or initialize the `config.textproto` from `data_dir`.
pub async fn load_or_init_config_textproto(
  data_dir: &DataDir,
  connection_manager: &ConnectionManager,
) -> Result<proto::Config, ConfigError> {
  let merged_config = {
    let config = match fs::read_to_string(config_path(data_dir)) {
      Ok(contents) => proto::Config::from_text(&contents)?,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
        warn!("`config.textproto` not found, initializing new default.");

        let config = proto::Config::new_with_custom_defaults();
        write_config_and_vault_textproto(data_dir, connection_manager, &config).await?;
        config
      }
      Err(err) => {
        return Err(err.into());
      }
    };

    let vault = load_vault_textproto_or_default(data_dir)?;
    merge_vault_and_env(config, vault)?
  };

  validate_config(connection_manager, &merged_config).await?;

  return Ok(merged_config);
}

fn split_config(config: &proto::Config) -> Result<(proto::Config, proto::Vault), ConfigError> {
  let mut new_vault = proto::Vault::default();
  let (stripped_config, secrets) = redact_secrets(config)?;

  for (key, value) in secrets {
    new_vault.secrets.insert(key, value);
  }

  return Ok((stripped_config, new_vault));
}

pub async fn write_config_and_vault_textproto(
  data_dir: &DataDir,
  connection_manager: &ConnectionManager,
  config: &proto::Config,
) -> Result<(), ConfigError> {
  validate_config(connection_manager, config).await?;

  let (stripped_config, vault) = split_config(config)?;

  if cfg!(test) {
    debug!("Skip writing config for tests.");
    return Ok(());
  }

  let config_path = config_path(data_dir);
  let vault_path = vault_path(data_dir);
  debug!("Writing config files: {config_path:?}, {vault_path:?}");
  fs::write(&config_path, stripped_config.to_text()?.as_bytes())?;
  fs::write(&vault_path, vault.to_text()?.as_bytes())?;
  return Ok(());
}
