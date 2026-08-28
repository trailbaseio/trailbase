use base64::prelude::*;
use chrono::Duration;
use prost::Message;
use prost_reflect::text_format::FormatOptions;
use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::LazyLock;

use crate::DESCRIPTOR_POOL;
use crate::config::textproto::{self, Textproto};
use crate::constants::{DEFAULT_AUTH_TOKEN_TTL, DEFAULT_REFRESH_TOKEN_TTL, LOGS_RETENTION_DEFAULT};

include!(concat!(env!("OUT_DIR"), "/config.rs"));

impl Vault {
  pub fn descriptor() -> MessageDescriptor {
    static VAULT_DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(|| {
      DESCRIPTOR_POOL
        .get_message_by_name("config.Vault")
        .expect("tested")
    });
    return VAULT_DESCRIPTOR.clone();
  }
}

impl Textproto<Vault> for Vault {
  fn from_text(text: &str) -> Result<Self, textproto::Error> {
    let dyn_config = DynamicMessage::parse_text_format(Self::descriptor(), text)?;
    return Ok(dyn_config.transcode_to::<Self>()?);
  }

  fn to_text(&self) -> Result<String, textproto::Error> {
    const PREAMBLE: &str = "# Auto-generated `config.Vault` textproto.\n#\n# Schema: https://github.com/trailbaseio/trailbase/blob/main/crates/core/proto/vault.proto";

    let options = FormatOptions::new().pretty(true).expand_any(true);
    let text: String = self
      .transcode_to_dynamic()
      .to_text_format_with_options(&options);

    return Ok(format!("{PREAMBLE}\n{text}"));
  }
}

impl Config {
  pub fn descriptor() -> MessageDescriptor {
    static CONFIG_DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(|| {
      DESCRIPTOR_POOL
        .get_message_by_name("config.Config")
        .expect("tested")
    });
    return CONFIG_DESCRIPTOR.clone();
  }

  pub fn new_with_custom_defaults() -> Self {
    // NOTE: It's arguable if copying custom defaults into the config is the cleanest approach,
    // however it lets us tie into the set update-config Admin UI flow to let users change the
    // templates.
    let config = Config {
      server: ServerConfig {
        application_name: Some("TrailBase".to_string()),
        site_url: None,
        logs_retention_sec: Some(LOGS_RETENTION_DEFAULT.num_seconds()),
        ..Default::default()
      },
      auth: AuthConfig {
        auth_token_ttl_sec: Some(DEFAULT_AUTH_TOKEN_TTL.num_seconds()),
        refresh_token_ttl_sec: Some(DEFAULT_REFRESH_TOKEN_TTL.num_seconds()),
        ..Default::default()
      },
      ..Default::default()
    };

    return config;
  }
}

impl Textproto<Config> for Config {
  fn from_text(text: &str) -> Result<Self, textproto::Error> {
    let dyn_config = DynamicMessage::parse_text_format(Self::descriptor(), text)?;
    return Ok(dyn_config.transcode_to::<Self>()?);
  }

  fn to_text(&self) -> Result<String, textproto::Error> {
    const PREAMBLE: &str = "# Auto-generated `config.Config` textproto.\n#\n# Schema: https://github.com/trailbaseio/trailbase/blob/main/crates/core/proto/config.proto";

    let options = FormatOptions::new().pretty(true).expand_any(true);
    let text: String = self
      .transcode_to_dynamic()
      .to_text_format_with_options(&options);

    return Ok(format!("{PREAMBLE}\n{text}"));
  }
}

impl GetConfigResponse {
  pub fn descriptor() -> MessageDescriptor {
    static DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(|| {
      DESCRIPTOR_POOL
        .get_message_by_name("config.GetConfigResponse")
        .expect("tested")
    });
    return DESCRIPTOR.clone();
  }
}

impl Textproto<GetConfigResponse> for GetConfigResponse {
  fn from_text(text: &str) -> Result<Self, textproto::Error> {
    let dyn_config = DynamicMessage::parse_text_format(Self::descriptor(), text)?;
    return Ok(dyn_config.transcode_to::<Self>()?);
  }

  fn to_text(&self) -> Result<String, textproto::Error> {
    let options = FormatOptions::new().pretty(true).expand_any(true);
    let text: String = self
      .transcode_to_dynamic()
      .to_text_format_with_options(&options);
    return Ok(text);
  }
}

impl UpdateConfigRequest {
  pub fn descriptor() -> MessageDescriptor {
    static DESCRIPTOR: LazyLock<MessageDescriptor> = LazyLock::new(|| {
      DESCRIPTOR_POOL
        .get_message_by_name("config.UpdateConfigRequest")
        .expect("tested")
    });
    return DESCRIPTOR.clone();
  }
}

impl Textproto<UpdateConfigRequest> for UpdateConfigRequest {
  fn from_text(text: &str) -> Result<Self, textproto::Error> {
    let dyn_config = DynamicMessage::parse_text_format(Self::descriptor(), text)?;
    return Ok(dyn_config.transcode_to::<Self>()?);
  }

  fn to_text(&self) -> Result<String, textproto::Error> {
    let options = FormatOptions::new().pretty(true).expand_any(true);
    let text: String = self
      .transcode_to_dynamic()
      .to_text_format_with_options(&options);
    return Ok(text);
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_get_descriptors() {
    // Make sure they don't panic.
    assert_eq!("config.Vault", Vault::descriptor().full_name());
    assert_eq!("config.Config", Config::descriptor().full_name());
    assert_eq!(
      "config.UpdateConfigRequest",
      UpdateConfigRequest::descriptor().full_name()
    );
    assert_eq!(
      "config.GetConfigResponse",
      GetConfigResponse::descriptor().full_name()
    );
  }
}
