use log::*;
use prost_reflect::{
  DynamicMessage, ExtensionDescriptor, FieldDescriptor, Kind, MapKey, ReflectMessage, Value,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::LazyLock;

use crate::DESCRIPTOR_POOL;
use crate::config::proto;
use crate::config::{ConfigError, Textproto};
use crate::data_dir::DataDir;

fn is_secret(field_descriptor: &FieldDescriptor) -> bool {
  static SECRET_EXT_DESCRIPTOR: LazyLock<ExtensionDescriptor> = LazyLock::new(|| {
    DESCRIPTOR_POOL
      .get_extension_by_name("config.secret")
      .expect("infallible")
  });

  let options = field_descriptor.options();
  if let Value::Bool(value) = *options.get_extension(&SECRET_EXT_DESCRIPTOR) {
    return value;
  }
  return false;
}

/// Merges settings from environment variables and secrets into the base msg/config.
///
/// NOTE: the merging semantics are different for env variables and secrets. The former are
/// overrides and will be set unconditionally, secrets will only be inserted for string fields
/// where the value is `PLACEHOLDER`. This allows changing secret values, w/o them getting
/// overridden when merging into a new config.
///
/// We could consider breaking the two up. We could even use serialized field descriptors as keys
/// in secrets fiel rather than env variable names.
fn recursively_merge_vault_and_env(
  msg: &mut DynamicMessage,
  vault: &proto::Vault,
  parent_path: Vec<String>,
) -> Result<(), ConfigError> {
  fn apply_parsed_env_var<T: std::str::FromStr>(
    msg: &mut DynamicMessage,
    field_desc: &FieldDescriptor,
    var_name: &str,
    f: impl Fn(T) -> prost_reflect::Value,
  ) -> Result<(), <T as std::str::FromStr>::Err> {
    if let Some(v) = parse_env_var::<T>(var_name)? {
      msg.set_field(field_desc, f(v));
    }
    Ok(())
  }

  for field_descr in msg.descriptor().fields() {
    let path = {
      let mut path = parent_path.clone();
      path.push(field_descr.name().to_uppercase());
      path
    };

    let var_name = format!("TRAIL_{path}", path = path.join("_"));
    let secret = is_secret(&field_descr);

    trace!("{var_name}: {secret}");

    match field_descr.kind() {
      Kind::Message(_) => {
        // FIXME: We're skipping missing optional message fields, which means potentially present
        // environment variables might not get merged. This is just a quick fix to avoid
        // instantiating new empty messages e.g. for email templates in EmailConfig :/.
        // This only ~works right now because most messages are required. Instead, we should lazily
        // construct sub-messages only when a corresponding env variable was found.
        //
        // In practice this often isn't too much of an issue, e.g. for oauth providers this means
        // we cannot merge the client_id_secret only if the client_id is set via env vars,
        // otherwise the message to merge into should already exist.
        if !msg.has_field(&field_descr) {
          debug!(
            "Unsupported: merging of secrets into uninitialized nested messages. Skipping: {}",
            field_descr.name()
          );
          continue;
        }

        match msg.get_field_mut(&field_descr) {
          Value::Message(child) => recursively_merge_vault_and_env(child, vault, path)?,
          Value::List(_child_list) => {
            // There isn't really a good way for us to support mapping env variables to repeated
            // fields. Hard-coding the index in the variable name sounds brittle. Instead, we just
            // don't support it.
            trace!("Skipping repeated field: {name}", name = field_descr.name());
            continue;
          }
          Value::Map(child_map) => {
            for (key, value) in child_map {
              match (key, value) {
                (MapKey::String(k), Value::Message(m)) => {
                  let mut keyed = path.clone();
                  keyed.push(k.to_uppercase());

                  recursively_merge_vault_and_env(m, vault, keyed)?
                }
                x => {
                  warn!("Unexpected message type: {x:?}");
                }
              }
            }
          }
          x => {
            warn!("Unexpected message type: {x:?}");
          }
        }
      }
      Kind::String => {
        // Env overrides takes priority letting user override any value whether from base config or
        // secrets.
        if let Some(value) = parse_env_var::<String>(&var_name).expect("infalliable") {
          msg.set_field(&field_descr, Value::String(value));
        } else if secret
          && let Value::String(ref field) = *msg.get_field(&field_descr)
          && field == PLACEHOLDER
        {
          // We found a secret field with a placeholder, we can expect a corresponding secret.
          let Some(stored_secret) = vault.secrets.get(&var_name) else {
            return Err(ConfigError::Invalid(format!(
              "Missing secret for: {path:?}"
            )));
          };

          msg.set_field(&field_descr, Value::String(stored_secret.clone()));
        }
      }
      Kind::Int32 => apply_parsed_env_var::<i32>(msg, &field_descr, &var_name, Value::I32)?,
      Kind::Uint32 => apply_parsed_env_var::<u32>(msg, &field_descr, &var_name, Value::U32)?,
      Kind::Int64 => apply_parsed_env_var::<i64>(msg, &field_descr, &var_name, Value::I64)?,
      Kind::Uint64 => apply_parsed_env_var::<u64>(msg, &field_descr, &var_name, Value::U64)?,
      Kind::Bool => apply_parsed_env_var::<bool>(msg, &field_descr, &var_name, Value::Bool)?,
      Kind::Enum(_) => {
        apply_parsed_env_var::<i32>(msg, &field_descr, &var_name, Value::EnumNumber)?
      }
      _ => {
        error!("Config merging not implemented for: {field_descr:?}");
      }
    };
  }

  return Ok(());
}

pub(crate) fn merge_vault_and_env(
  config: proto::Config,
  vault: proto::Vault,
) -> Result<proto::Config, ConfigError> {
  let mut dyn_config = config.transcode_to_dynamic();
  recursively_merge_vault_and_env(&mut dyn_config, &vault, vec![])?;
  return Ok(dyn_config.transcode_to::<proto::Config>()?);
}

fn recursively_redact_secrets(
  msg: &mut DynamicMessage,
  secrets: &mut HashMap<String, String>,
  parent_path: Vec<String>,
) -> Result<(), ConfigError> {
  for field_descr in msg.descriptor().fields() {
    // If the field is empty, there's nothing to redact.
    if !msg.has_field(&field_descr) {
      continue;
    }

    let path = {
      let mut path = parent_path.clone();
      path.push(field_descr.name().to_uppercase());
      path
    };

    let secret = is_secret(&field_descr);

    match msg.get_field_mut(&field_descr) {
      Value::Message(child) => recursively_redact_secrets(child, secrets, path)?,
      Value::Map(child_map) => {
        for (key, value) in child_map {
          match (key, value) {
            (MapKey::String(k), Value::Message(m)) => {
              // NOTE: We're pushing a second time here, making the path segment:
              // "<FIELD_NAME>_<MAP_KEY>".
              let mut keyed = path.clone();
              keyed.push(k.to_uppercase());

              recursively_redact_secrets(m, secrets, keyed)?
            }
            x => {
              warn!("Unexpected message type: {x:?}");
            }
          }
        }
      }
      Value::String(field) => {
        if secret {
          // Insert into map.
          secrets.insert(
            format!("TRAIL_{path}", path = path.join("_")),
            field.clone(),
          );

          // Then redact the field.
          msg.set_field(&field_descr, Value::String(PLACEHOLDER.to_string()));
        }
      }
      x => {
        if secret {
          error!("Found non-string secret. Not supported: {x:?}");
        }
      }
    }
  }

  return Ok(());
}

pub(crate) fn redact_secrets(
  config: &proto::Config,
) -> Result<(proto::Config, HashMap<String, String>), ConfigError> {
  let mut secrets = HashMap::<String, String>::new();
  let mut dyn_config = config.transcode_to_dynamic();
  recursively_redact_secrets(&mut dyn_config, &mut secrets, vec![])?;
  let stripped = dyn_config.transcode_to::<proto::Config>()?;

  return Ok((stripped, secrets));
}

pub(crate) fn vault_path(data_dir: &DataDir) -> PathBuf {
  const VAULT_FILENAME: &str = "secrets.textproto";
  return data_dir.secrets_path().join(VAULT_FILENAME);
}

pub(crate) fn load_vault_textproto_or_default(
  data_dir: &DataDir,
) -> Result<proto::Vault, ConfigError> {
  let vault_path = vault_path(data_dir);

  let vault = match fs::read_to_string(&vault_path) {
    Ok(contents) => proto::Vault::from_text(&contents)?,
    Err(err) => {
      if cfg!(not(test)) {
        warn!("Vault not found. Falling back to empty default vault: {err}");
      }
      proto::Vault {
        ..Default::default()
      }
    }
  };

  return Ok(vault);
}

#[cfg(not(test))]
fn parse_env_var<T: std::str::FromStr>(
  name: &str,
) -> Result<Option<T>, <T as std::str::FromStr>::Err> {
  if let Ok(value) = std::env::var(name) {
    return Ok(Some(value.parse::<T>()?));
  }
  Ok(None)
}

#[cfg(test)]
use tests::parse_env_var;

const PLACEHOLDER: &str = "<REDACTED>";

#[cfg(test)]
mod tests {
  use std::cell::RefCell;
  use std::collections::HashMap;

  use super::*;

  thread_local! {
    static ENV: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
  }

  pub fn parse_env_var<T: std::str::FromStr>(
    name: &str,
  ) -> Result<Option<T>, <T as std::str::FromStr>::Err> {
    if let Some(value) = ENV.with(|e| e.borrow().get(name).cloned()) {
      return Ok(Some(value.parse::<T>()?));
    }

    return Ok(None);
  }

  pub fn env_set(name: &str, value: Option<&str>) {
    match value {
      None => ENV.with(|e| e.borrow_mut().remove(name)),
      Some(v) => ENV.with(|e| e.borrow_mut().insert(name.to_string(), v.to_string())),
    };
  }

  pub fn env_clear() {
    ENV.with(|e| e.borrow_mut().clear());
  }

  #[test]
  fn test_config_stripping() {
    let config = proto::Config {
      email: proto::EmailConfig {
        smtp_username: Some("user".to_string()),
        smtp_password: Some("pass".to_string()),
        ..Default::default()
      },
      auth: proto::AuthConfig {
        oauth_providers: HashMap::from([(
          "key".to_string(),
          proto::OAuthProviderConfig {
            client_id: Some("my_client_id".to_string()),
            client_secret: Some("secret".to_string()),
            ..Default::default()
          },
        )]),
        ..Default::default()
      },
      ..Default::default()
    };

    let expected = {
      let mut expected = config.clone();
      // Redact field
      expected.email.smtp_password = Some(PLACEHOLDER.to_string());
      // Redact map entry.
      expected
        .auth
        .oauth_providers
        .get_mut("key")
        .unwrap()
        .client_secret = Some(PLACEHOLDER.to_string());
      expected
    };

    let (stripped, secrets) = redact_secrets(&config).unwrap();
    assert_eq!(stripped, expected);
    assert_eq!(
      secrets.get("TRAIL_EMAIL_SMTP_PASSWORD"),
      Some(&"pass".to_string())
    );
    assert_eq!(
      secrets.get("TRAIL_AUTH_OAUTH_PROVIDERS_KEY_CLIENT_SECRET"),
      Some(&"secret".to_string())
    );
  }

  #[test]
  fn test_config_merging_from_env_and_vault() {
    // Set username via env var.
    let username = "secret_username";
    env_set("TRAIL_EMAIL_SMTP_USERNAME", Some(username));

    let password = "secret_password";
    let client_secret = "secret".to_string();
    let outh_map_key = "fake_provider";
    let vault = proto::Vault {
      secrets: HashMap::<String, String>::from([
        (
          "TRAIL_EMAIL_SMTP_PASSWORD".to_string(),
          password.to_string(),
        ),
        (
          format!(
            "TRAIL_AUTH_OAUTH_PROVIDERS_{}_CLIENT_SECRET",
            outh_map_key.to_uppercase()
          ),
          client_secret.clone(),
        ),
        (
          format!("TRAIL_AUTH_OAUTH_PROVIDERS_MISSING_CLIENT_SECRET"),
          "SHOULD NOT BE SET".to_string(),
        ),
      ]),
    };

    let config = proto::Config {
      email: proto::EmailConfig {
        smtp_username: Some(PLACEHOLDER.to_string()),
        smtp_password: Some(PLACEHOLDER.to_string()),
        ..Default::default()
      },
      auth: proto::AuthConfig {
        oauth_providers: HashMap::from([(
          outh_map_key.to_string(),
          proto::OAuthProviderConfig {
            client_id: Some("my_client_id".to_string()),
            client_secret: Some(PLACEHOLDER.to_string()),
            ..Default::default()
          },
        )]),
        ..Default::default()
      },
      ..Default::default()
    };

    let merged = merge_vault_and_env(config.clone(), vault).unwrap();
    env_clear();

    // Build expected config with secrets.
    let expected = {
      let mut expected = config.clone();
      expected.email = proto::EmailConfig {
        smtp_username: Some(username.to_string()),
        smtp_password: Some(password.to_string()),
        ..Default::default()
      };
      expected
        .auth
        .oauth_providers
        .get_mut(outh_map_key)
        .unwrap()
        .client_secret = Some(client_secret);

      expected
    };

    assert_eq!(merged, expected);
  }

  #[test]
  fn test_config_merging() {
    let config = proto::Config {
      email: proto::EmailConfig {
        smtp_username: Some("user".to_string()),
        ..Default::default()
      },
      ..Default::default()
    };
    let vault = proto::Vault::default();
    let merged = merge_vault_and_env(config.clone(), vault).unwrap();

    assert_eq!(config, merged);
  }

  #[test]
  fn test_strip_and_merge() {
    let config = proto::Config {
      email: proto::EmailConfig {
        smtp_username: Some("secret_username".to_string()),
        smtp_password: Some("secret_password".to_string()),
        ..Default::default()
      },
      auth: proto::AuthConfig {
        oauth_providers: HashMap::from([(
          "fake_provider".to_string(),
          proto::OAuthProviderConfig {
            client_id: Some("my_client_id".to_string()),
            client_secret: Some("secret_client_secret".to_string()),
            ..Default::default()
          },
        )]),
        ..Default::default()
      },
      ..Default::default()
    };

    let (stripped, secrets) = redact_secrets(&config).unwrap();
    let vault = proto::Vault { secrets };
    let merged = merge_vault_and_env(stripped, vault).unwrap();

    assert_eq!(config, merged);
  }
}
