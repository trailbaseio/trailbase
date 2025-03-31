use lazy_static::lazy_static;
use prost_reflect::{
  DynamicMessage, ExtensionDescriptor, FieldDescriptor, Kind, MapKey, ReflectMessage, Value,
};
use proto::{EmailTemplate, OAuthProviderId};
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::str::FromStr;
use thiserror::Error;
use tokio::fs;
use tracing::*;
use validator::{ValidateEmail, ValidateUrl};

use crate::auth::oauth::providers::oauth_provider_registry;
use crate::data_dir::DataDir;
use crate::records::validate_record_api_config;
use crate::table_metadata::TableMetadataCache;
use crate::DESCRIPTOR_POOL;

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error("Decode error: {0}")]
  Decode(#[from] prost::DecodeError),
  #[error("Parse error: {0}")]
  Parse(#[from] prost_reflect::text_format::ParseError),
  #[error("Parse int error: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("Parse bool error: {0}")]
  ParseBool(#[from] std::str::ParseBoolError),
  #[error("Valiation error: {0}")]
  Invalid(String),
  #[error("Update error: {0}")]
  Update(String),
  #[error("IO error: {0}")]
  IO(#[from] std::io::Error),
  #[error("Id error: {0}")]
  Id(#[from] uuid::Error),
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
use test_env::parse_env_var;

pub(super) fn apply_parsed_env_var<T: std::str::FromStr>(
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

pub mod proto {
  use base64::prelude::*;
  use chrono::Duration;
  use lazy_static::lazy_static;
  use prost::Message;
  use prost_reflect::text_format::FormatOptions;
  use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage};
  use std::hash::{DefaultHasher, Hash, Hasher};

  use crate::config::ConfigError;
  use crate::constants::{
    AVATAR_TABLE, DEFAULT_AUTH_TOKEN_TTL, DEFAULT_REFRESH_TOKEN_TTL, LOGS_RETENTION_DEFAULT,
  };
  use crate::email;
  use crate::DESCRIPTOR_POOL;

  include!(concat!(env!("OUT_DIR"), "/config.rs"));

  lazy_static! {
    static ref CONFIG_DESCRIPTOR: MessageDescriptor = DESCRIPTOR_POOL
      .get_message_by_name("config.Config")
      .expect("infallible");
    static ref VAULT_DESCRIPTOR: MessageDescriptor = DESCRIPTOR_POOL
      .get_message_by_name("config.Vault")
      .expect("infallible");
    static ref FORMAT_OPTIONS: FormatOptions = FormatOptions::new().pretty(true).expand_any(true);
  }

  impl Vault {
    pub fn from_text(text: &str) -> Result<Self, ConfigError> {
      let dyn_config = DynamicMessage::parse_text_format(VAULT_DESCRIPTOR.clone(), text)?;
      return Ok(dyn_config.transcode_to::<Self>()?);
    }

    pub fn to_text(&self) -> Result<String, ConfigError> {
      const PREFACE: &str = "# Auto-generated config.Vault textproto";

      let text: String = self
        .transcode_to_dynamic()
        .to_text_format_with_options(&FORMAT_OPTIONS);

      return Ok(format!("{PREFACE}\n{text}"));
    }
  }

  impl Config {
    pub fn new_with_custom_defaults() -> Self {
      // NOTE: It's arguable if copying custom defaults into the config is the cleanest approach,
      // however it lets us tie into the set update-config Admin UI flow to let users change the
      // templates.
      let mut config = Config {
        server: ServerConfig {
          application_name: Some("TrailBase".to_string()),
          site_url: None,
          logs_retention_sec: Some(LOGS_RETENTION_DEFAULT.num_seconds()),
          ..Default::default()
        },
        email: EmailConfig {
          user_verification_template: Some(email::defaults::email_validation_email()),
          password_reset_template: Some(email::defaults::password_reset_email()),
          change_email_template: Some(email::defaults::change_email_address_email()),
          ..Default::default()
        },
        auth: AuthConfig {
          auth_token_ttl_sec: Some(DEFAULT_AUTH_TOKEN_TTL.num_seconds()),
          refresh_token_ttl_sec: Some(DEFAULT_REFRESH_TOKEN_TTL.num_seconds()),
          ..Default::default()
        },
        ..Default::default()
      };

      config.record_apis = vec![RecordApiConfig {
        name: Some(AVATAR_TABLE.to_string()),
        table_name: Some(AVATAR_TABLE.to_string()),
        conflict_resolution: Some(ConflictResolutionStrategy::Replace.into()),
        autofill_missing_user_id_columns: Some(true),
        enable_subscriptions: None,
        acl_world: vec![PermissionFlag::Read as i32],
        acl_authenticated: vec![
          PermissionFlag::Create as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Update as i32,
          PermissionFlag::Delete as i32,
        ],
        read_access_rule: None,
        create_access_rule: Some("_REQ_.user IS NULL OR _REQ_.user = _USER_.id".to_string()),
        update_access_rule: Some("_ROW_.user = _USER_.id".to_string()),
        delete_access_rule: Some("_ROW_.user = _USER_.id".to_string()),
        schema_access_rule: None,
        expand: vec![],
      }];

      return config;
    }

    pub fn from_text(text: &str) -> Result<Self, ConfigError> {
      let dyn_config = DynamicMessage::parse_text_format(CONFIG_DESCRIPTOR.clone(), text)?;
      return Ok(dyn_config.transcode_to::<Self>()?);
    }

    pub fn to_text(&self) -> Result<String, ConfigError> {
      const PREFACE: &str = "# Auto-generated config.Config textproto";

      let text: String = self
        .transcode_to_dynamic()
        .to_text_format_with_options(&FORMAT_OPTIONS);

      return Ok(format!("{PREFACE}\n{text}"));
    }
  }

  impl AuthConfig {
    pub fn token_ttls(&self) -> (Duration, Duration) {
      return (
        self
          .auth_token_ttl_sec
          .map_or(DEFAULT_AUTH_TOKEN_TTL, Duration::seconds),
        self
          .refresh_token_ttl_sec
          .map_or(DEFAULT_REFRESH_TOKEN_TTL, Duration::seconds),
      );
    }
  }

  pub fn hash_config(config: &Config) -> String {
    let encoded = config.encode_to_vec();
    let mut s = DefaultHasher::new();
    encoded.hash(&mut s);
    let hash = s.finish();

    return BASE64_URL_SAFE.encode(hash.to_le_bytes());
  }
}

fn is_secret(field_descriptor: &FieldDescriptor) -> bool {
  lazy_static! {
    static ref SECRET_EXT_DESCRIPTOR: ExtensionDescriptor = DESCRIPTOR_POOL
      .get_extension_by_name("config.secret")
      .expect("infallible");
  }

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
        } else if secret {
          if let Value::String(ref field) = *msg.get_field(&field_descr) {
            if field == PLACEHOLDER {
              // We found a secret field with a placeholder, we can expect a corresponding secret.
              let Some(stored_secret) = vault.secrets.get(&var_name) else {
                return Err(ConfigError::Invalid(format!(
                  "Missing secret for: {path:?}"
                )));
              };

              msg.set_field(&field_descr, Value::String(stored_secret.clone()));
            }
          }
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

const PLACEHOLDER: &str = "<REDACTED>";

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

async fn load_vault_textproto_or_default(data_dir: &DataDir) -> Result<proto::Vault, ConfigError> {
  let vault_path = data_dir.secrets_path().join(VAULT_FILENAME);

  let vault = match fs::read_to_string(&vault_path).await {
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

pub async fn load_or_init_config_textproto(
  data_dir: &DataDir,
  table_metadata: &TableMetadataCache,
) -> Result<proto::Config, ConfigError> {
  let vault = load_vault_textproto_or_default(data_dir).await?;

  let config: proto::Config =
    match fs::read_to_string(data_dir.config_path().join(CONFIG_FILENAME)).await {
      Ok(contents) => proto::Config::from_text(&contents)?,
      Err(err) => match err.kind() {
        std::io::ErrorKind::NotFound => {
          warn!("Falling back to default config: {err}");
          let config = proto::Config::new_with_custom_defaults();
          write_config_and_vault_textproto(data_dir, table_metadata, &config).await?;
          config
        }
        _ => {
          return Err(err.into());
        }
      },
    };

  let merged_config = merge_vault_and_env(config, vault)?;
  validate_config(table_metadata, &merged_config)?;

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
  table_metadata: &TableMetadataCache,
  config: &proto::Config,
) -> Result<(), ConfigError> {
  validate_config(table_metadata, config)?;

  let (stripped_config, vault) = split_config(config)?;

  if cfg!(test) {
    debug!("Skip writing config for tests.");
    return Ok(());
  }

  let config_path = data_dir.config_path().join(CONFIG_FILENAME);
  let vault_path = data_dir.secrets_path().join(VAULT_FILENAME);
  debug!("Writing config files: {config_path:?}, {vault_path:?}");
  fs::write(&config_path, stripped_config.to_text()?.as_bytes()).await?;
  fs::write(&vault_path, vault.to_text()?.as_bytes()).await?;
  return Ok(());
}

fn validate_application_name(name: &str) -> Result<(), ConfigError> {
  if !name
    .chars()
    .all(|x| x.is_ascii_alphanumeric() || x == '_' || x == '.' || x == '-' || x == ' ')
  {
    return Err(ConfigError::Invalid(format!(
      "Application name: {name}. Must only contain alphanumeric characters, spaces or '_', '-', '.'."
    )));
  }

  if name.is_empty() {
    return Err(ConfigError::Invalid(
      "Application name must not be empty".to_string(),
    ));
  }

  Ok(())
}

pub(crate) fn validate_config(
  tables: &TableMetadataCache,
  config: &proto::Config,
) -> Result<(), ConfigError> {
  fn ierr(msg: impl Into<String>) -> Result<(), ConfigError> {
    return Err(ConfigError::Invalid(msg.into()));
  }

  let Some(app_name) = &config.server.application_name else {
    return ierr("Missing application name");
  };
  validate_application_name(app_name)?;

  // Check RecordApis.
  //
  // Note: it is valid to declare multiple api (e.g. with different acls) over the same
  // table, however it's not valid to have conflicting api names.
  let mut api_names = HashSet::<String>::new();
  for api in &config.record_apis {
    let api_name = validate_record_api_config(tables, api)?;

    if !api_names.insert(api_name.clone()) {
      return ierr(format!(
        "Two or more APIs have the colliding name: '{api_name}'"
      ));
    }
  }

  // Check OAuth.
  for (name, provider) in &config.auth.oauth_providers {
    let provider_id: OAuthProviderId = provider
      .provider_id
      .unwrap_or(0)
      .try_into()
      .map_err(|_| ConfigError::Invalid("Invalid provider id".into()))?;
    if provider_id == OAuthProviderId::OauthProviderIdUndefined {
      return ierr(format!("Invalid id for provider: {name}"));
    }

    let Some(factory) = oauth_provider_registry
      .iter()
      .find(|factory| factory.id == provider_id)
    else {
      return ierr(format!("Missing factory for: {name}"));
    };

    if name != factory.factory_name {
      return ierr(format!("Factory name mismatch for: {name}"));
    }

    if provider.client_secret.is_none() {
      return ierr(format!("Missing secret for: {name}"));
    }

    if provider.client_id.is_none() {
      return ierr(format!("Missing client id for: {name}"));
    }

    if provider_id == OAuthProviderId::Oidc0 {
      if !provider.auth_url.validate_url() {
        return ierr(format!("Invalid auth url for: {name}"));
      }

      if !provider.token_url.validate_url() {
        return ierr(format!("Invalid token url for: {name}"));
      }

      if !provider.user_api_url.validate_url() {
        return ierr(format!("Invalid user api url for: {name}"));
      }
    }
  }

  // Check JSON Schema configs
  for schema in &config.schemas {
    if schema.name.is_none() {
      return ierr("Missing schema name");
    }

    let Some(schema_text) = &schema.schema else {
      return ierr("Missing schema");
    };

    let schema_json: serde_json::Value = serde_json::from_str(schema_text)
      .map_err(|err| ConfigError::Invalid(format!("Schema is invalid Json: {err}")))?;
    if let Err(err) = jsonschema::meta::validate(&schema_json) {
      return Err(ConfigError::Invalid(format!(
        "Not a valid Json schema: {err}"
      )));
    }
  }

  // Check email config.
  {
    let email = &config.email;

    let mut num_smtp_fields = 0;
    if let Some(ref host) = email.smtp_host {
      if !format!("http://{host}/").validate_url() {
        return ierr(format!("Invalid SMTP host {host}."));
      }
      num_smtp_fields += 1;
    }

    if let Some(port) = email.smtp_port {
      let port = u16::try_from(port).map_err(|_| ConfigError::Invalid("not a u16".into()))?;
      if port == 0 {
        return ierr("Invalid SMTP port.");
      }
      num_smtp_fields += 1;
    }

    if let Some(ref username) = email.smtp_username {
      if username.is_empty() {
        return ierr("Invalid SMTP username.");
      }
      num_smtp_fields += 1;
    }

    if let Some(ref password) = email.smtp_password {
      if password.is_empty() {
        return ierr("Invalid SMTP username.");
      }
      num_smtp_fields += 1;
    }

    if num_smtp_fields != 0 && num_smtp_fields != 4 {
      return ierr("Only a subset of SMTP settings provided");
    }

    if let Some(ref sender_address) = email.sender_address {
      if !sender_address.validate_email() {
        return ierr("Invalid sender address.");
      };
      if email.sender_name.is_none() {
        return ierr("Sender address but missing sender name.");
      }
    }

    let validate_template = |template: Option<&EmailTemplate>| {
      if let Some(template) = template {
        if template.subject.is_none() || template.body.is_none() {
          return ierr("Email template missing subject or body.");
        }
      };
      Ok(())
    };

    validate_template(email.user_verification_template.as_ref())?;
    validate_template(email.change_email_template.as_ref())?;
    validate_template(email.password_reset_template.as_ref())?;
  }

  // Check job config.
  for job in &config.jobs.system_jobs {
    let Some(ref id) = job.id else {
      return ierr("Job is missing id.");
    };

    let Some(ref schedule) = job.schedule else {
      return ierr(format!("Job '{id}' is missing schedule."));
    };

    if let Err(err) = cron::Schedule::from_str(schedule) {
      return ierr(format!("Schedule of job '{id}' not valid cron: {err}"));
    }
  }

  return Ok(());
}

#[cfg(test)]
mod test_env {
  use lazy_static::lazy_static;
  use parking_lot::Mutex;
  use std::collections::HashMap;

  lazy_static! {
    pub static ref ENV: Mutex<HashMap<String, String>> = Mutex::new(HashMap::new());
  }

  pub fn parse_env_var<T: std::str::FromStr>(
    name: &str,
  ) -> Result<Option<T>, <T as std::str::FromStr>::Err> {
    if let Some(value) = ENV.lock().get(name) {
      return Ok(Some(value.parse::<T>()?));
    }
    Ok(None)
  }

  pub fn set(name: &str, value: Option<&str>) {
    match value {
      None => ENV.lock().remove(name),
      Some(v) => ENV.lock().insert(name.to_string(), v.to_string()),
    };
  }

  pub fn clear() {
    ENV.lock().clear();
  }
}

#[cfg(test)]
mod test {
  use std::collections::HashMap;

  use super::*;
  use crate::app_state::test_state;
  use crate::config::proto::{AuthConfig, Config, EmailConfig, OAuthProviderConfig};

  #[tokio::test]
  async fn test_config_tests_sequentially() {
    // Run sequentially to avoid concurrent tests clobbering their env variables.
    test_default_config_is_valid().await;
    test_config_merging();
    test_config_stripping();
    test_config_merging_from_env_and_vault();
    test_strip_and_merge();
  }

  async fn test_default_config_is_valid() {
    let state = test_state(None).await.unwrap();
    let table_metadata = TableMetadataCache::new(state.conn().clone()).await.unwrap();

    let config = Config::new_with_custom_defaults();
    validate_config(&table_metadata, &config).unwrap();
  }

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

  fn test_config_stripping() {
    let config = proto::Config {
      email: proto::EmailConfig {
        smtp_username: Some("user".to_string()),
        smtp_password: Some("pass".to_string()),
        ..Default::default()
      },
      auth: proto::AuthConfig {
        oauth_providers: HashMap::<String, proto::OAuthProviderConfig>::from([(
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

  fn test_config_merging_from_env_and_vault() {
    // Set username via env var.
    let username = "secret_username";
    test_env::set("TRAIL_EMAIL_SMTP_USERNAME", Some(username));

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
      email: EmailConfig {
        smtp_username: Some(PLACEHOLDER.to_string()),
        smtp_password: Some(PLACEHOLDER.to_string()),
        ..Default::default()
      },
      auth: AuthConfig {
        oauth_providers: HashMap::<String, OAuthProviderConfig>::from([(
          outh_map_key.to_string(),
          OAuthProviderConfig {
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
    test_env::clear();

    // Build expected config with secrets.
    let expected = {
      let mut expected = config.clone();
      expected.email = EmailConfig {
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

  fn test_strip_and_merge() {
    let config = proto::Config {
      email: EmailConfig {
        smtp_username: Some("secret_username".to_string()),
        smtp_password: Some("secret_password".to_string()),
        ..Default::default()
      },
      auth: AuthConfig {
        oauth_providers: HashMap::<String, OAuthProviderConfig>::from([(
          "fake_provider".to_string(),
          OAuthProviderConfig {
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

const CONFIG_FILENAME: &str = "config.textproto";
const VAULT_FILENAME: &str = "secrets.textproto";
