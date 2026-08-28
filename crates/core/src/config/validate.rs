use log::*;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::str::FromStr;
use trailbase_sqlite::ConnectionType;
use validator::{ValidateEmail, ValidateUrl};

use crate::auth::oauth::providers::oauth_providers_static_registry;
use crate::config::{ConfigError, proto};
use crate::connection::ConnectionManager;
use crate::records::validate_record_api_config;

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

pub async fn validate_config(
  connection_manager: &ConnectionManager,
  config: &proto::Config,
) -> Result<(), ConfigError> {
  // Check server settings.
  let Some(ref app_name) = config.server.application_name else {
    return ierr("Missing application name");
  };
  validate_application_name(app_name)?;

  let site_url = match config.server.site_url {
    Some(ref site_url) => Some(url::Url::parse(site_url).map_err(|err| {
      ConfigError::Invalid(format!("Failed to parse site_url '{site_url}': {err}",))
    })?),
    None => None,
  };

  let connection_type = connection_manager.main_entry().connection.connection_type();

  let mut db_names = HashSet::<String>::new();
  for db in &config.databases {
    if matches!(connection_type, ConnectionType::Pg) {
      return ierr("PG doesn't (yet) support multiple DBs.");
    }

    let Some(ref name) = db.name else {
      return ierr("Missing database name");
    };

    if !db_names.insert(name.clone()) {
      return ierr(format!("Database '{name}' linked more than once"));
    }

    match name.as_str() {
      "main" | "public" | "logs" | "session" | "" => {
        return ierr(format!("Invalid database name: {name}"));
      }
      name
        if !name
          .chars()
          .all(|x| x.is_ascii_alphanumeric() || x == '_' || x == '-') =>
      {
        return ierr(format!("Invalid database name: {name}"));
      }
      _ => {}
    }
  }

  // Check RecordApis.
  //
  // Note: it is valid to declare multiple api (e.g. with different acls) over the same
  // table, however it's not valid to have conflicting api names.
  let mut api_names = HashSet::<String>::new();
  for api in &config.record_apis {
    let api_name = validate_record_api_config(connection_manager, api, &config.databases).await?;

    if !api_names.insert(api_name.clone()) {
      return ierr(format!(
        "Two or more APIs have the colliding name: '{api_name}'"
      ));
    }
  }

  // Check OAuth.
  if !config.auth.oauth_providers.is_empty() && site_url.is_none() {
    info!(
      "OAuth requires a public URL for redirects from external auth providers but `config.server.site_url` not set. May have been provided via `--public-url` instead"
    );
  }

  for (name, provider) in &config.auth.oauth_providers {
    let provider_id: proto::OAuthProviderId = provider
      .provider_id
      .unwrap_or(0)
      .try_into()
      .map_err(|_| ConfigError::Invalid("Invalid provider id".into()))?;
    if provider_id == proto::OAuthProviderId::OauthProviderIdUndefined {
      return ierr(format!("Invalid id for provider: {name}"));
    }

    let Some(factory) = oauth_providers_static_registry()
      .iter()
      .find(|factory| factory.id == provider_id)
    else {
      return ierr(format!("Missing factory for: {name}"));
    };

    if name != factory.factory_name {
      return ierr(format!("Factory name mismatch for: {name}"));
    }

    if let Some(ref client_id) = provider.client_id {
      if client_id != client_id.trim() {
        return ierr(format!(
          "OAuth provider {name}'s client id contains unexpected whitespaces"
        ));
      }
    } else {
      return ierr(format!("Missing client id for: {name}"));
    }

    if let Some(ref client_secret) = provider.client_secret {
      if client_secret != client_secret.trim() {
        return ierr(format!(
          "OAuth provider {name}'s client secret contains unexpected whitespaces"
        ));
      }
    } else {
      return ierr(format!("Missing secret for: {name}"));
    }

    if provider_id == proto::OAuthProviderId::Oidc0 {
      if provider
        .auth_url
        .as_ref()
        .as_ref()
        .is_none_or(|url| !url.validate_url())
      {
        return ierr(format!("Invalid auth url for: {name}"));
      }

      if provider
        .token_url
        .as_ref()
        .is_none_or(|url| !url.validate_url())
      {
        return ierr(format!("Invalid token url for: {name}"));
      }

      if provider
        .user_api_url
        .as_ref()
        .is_none_or(|url| !url.validate_url())
      {
        return ierr(format!("Invalid user api url for '{name}"));
      }
    }
  }

  // Check JSON Schema configs
  for schema in &config.schemas {
    if matches!(connection_type, ConnectionType::Pg) {
      return ierr("PG doesn't (yet) support custom schemas.");
    }

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
  validate_email_config(&config.email)?;

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

fn is_valid_hostname_or_ip(host: &str) -> bool {
  return url::Host::parse(host).is_ok();
}

pub(crate) fn validate_email_config(email: &proto::EmailConfig) -> Result<(), ConfigError> {
  validate_email_template(
    email.user_verification_template.as_ref(),
    &["VERIFICATION_URL", "CODE", "TOKEN"],
  )?;
  validate_email_template(
    email.change_email_template.as_ref(),
    &["VERIFICATION_URL", "CODE", "TOKEN"],
  )?;
  validate_email_template(email.password_reset_template.as_ref(), &["TOKEN", "CODE"])?;
  validate_email_template(email.otp_template.as_ref(), &["CODE"])?;

  let Some(host) = &email.smtp_host else {
    match (email.smtp_port, &email.smtp_username, &email.smtp_password) {
      (None, None, None) => {
        // No SMTP configured
        return Ok(());
      }
      _ => {
        return ierr("Partial SMTP configuration provided. Host missing.");
      }
    }
  };

  if !is_valid_hostname_or_ip(host) {
    return ierr(format!("SMTP host '{host}' is invalid."));
  }

  // NOTE: When no explicit sender is given, we fall back to noreply@host.
  if let Some(ref sender_address) = email.sender_address {
    if !sender_address.validate_email() {
      return ierr("Invalid sender address.");
    };
    if email.sender_name.is_none() {
      return ierr("Sender address but missing sender name.");
    }
  }

  let _port: u16 = match email.smtp_port {
    Some(port) => {
      // NOTE: Protobuf doesn't support uint16 types natively, so we have to range-check.
      let port = u16::try_from(port).map_err(|_| ConfigError::Invalid("not a u16".into()))?;
      if port == 0 {
        return ierr("Invalid SMTP port.");
      }
      port
    }
    None => {
      return ierr("SMTP port missing.");
    }
  };

  let user = &email.smtp_username;
  let pw = &email.smtp_password;

  return match email.smtp_encryption() {
    proto::SmtpEncryption::None => Ok(()),
    _enc => {
      if let Some(user) = user {
        if user.is_empty() {
          return ierr("Invalid SMTP username.");
        }
      } else {
        return ierr("Missing SMTP username.");
      }

      if let Some(pw) = pw {
        if pw.is_empty() {
          return ierr("Invalid SMTP username.");
        }
      } else {
        return ierr("Missing SMTP password.");
      }

      Ok(())
    }
  };
}

fn validate_email_template(
  template: Option<&proto::EmailTemplate>,
  acceptable_vars: &[&str],
) -> Result<(), ConfigError> {
  // NOTE: It's ok for either subject or body to be empty, we'll simply fall back to the
  // defaults.

  // Check that at least one of the acceptable template variables is present.
  if let Some(ref body) = template.as_ref().and_then(|t| t.body.as_ref()) {
    let any_match = acceptable_vars.iter().any(|var| {
      let pattern = format!(r"\{{\{{[ ]*{}[ ]*\}}\}}", var);
      regex::Regex::new(&pattern).expect("static").is_match(body)
    });

    if !any_match {
      return ierr(format!(
        "Body needs to contain one of: {vars}. Got: {body}",
        vars = acceptable_vars.join(", "),
      ));
    }
  }

  return Ok(());
}

fn ierr(msg: impl std::string::ToString) -> Result<(), ConfigError> {
  return Err(ConfigError::Invalid(msg.to_string()));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;
  use crate::config::proto;

  #[tokio::test]
  async fn test_default_config_is_valid() {
    let state = test_state(None).await.unwrap();

    let config = proto::Config::new_with_custom_defaults();
    validate_config(&state.connection_manager(), &config)
      .await
      .unwrap();
  }

  #[test]
  fn test_is_valid_hostname_or_ip() {
    assert_eq!(false, is_valid_hostname_or_ip(""));
    assert_eq!(true, is_valid_hostname_or_ip("0.0.0.0"));
    assert_eq!(true, is_valid_hostname_or_ip("smtp.test.org"));
    assert_eq!(false, is_valid_hostname_or_ip("smtp.test.org:4444"));
    assert_eq!(false, is_valid_hostname_or_ip("http://example.com"));
  }

  #[test]
  fn test_validate_email_template_password_reset_with_token_passes() {
    let template = proto::EmailTemplate {
      body: Some("Your token is: {{ TOKEN }}".to_string()),
      ..Default::default()
    };
    validate_email_template(Some(&template), &["TOKEN"]).unwrap();
  }

  #[test]
  fn test_validate_email_template_password_reset_with_code_passes() {
    let template = proto::EmailTemplate {
      body: Some("Your code is: {{ CODE }}".to_string()),
      ..Default::default()
    };
    validate_email_template(Some(&template), &["TOKEN", "CODE"]).unwrap();
  }

  #[test]
  fn test_validate_email_template_without_acceptable_var_fails() {
    let template = proto::EmailTemplate {
      body: Some("Just a static message with no params.".to_string()),
      ..Default::default()
    };
    let err = validate_email_template(Some(&template), &["TOKEN", "CODE"]).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("TOKEN"), "error must list TOKEN, got: {msg}");
    assert!(msg.contains("CODE"), "error must list CODE, got: {msg}");
  }

  #[test]
  fn test_validate_email_template_empty_or_none_body_passes() {
    validate_email_template(None, &["TOKEN"]).unwrap();
    let template = proto::EmailTemplate {
      body: None,
      ..Default::default()
    };
    validate_email_template(Some(&template), &["TOKEN"]).unwrap();
  }
}
