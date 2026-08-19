use scopeguard::defer;
use url::Url;

use crate::auth::AuthError;
use crate::auth::oauth::provider::{OAuthClient, OAuthProvider};
use crate::auth::oauth::providers::oauth_providers_static_registry;
use crate::auth::password::PasswordOptions;
use crate::config::proto::AuthConfig;

pub struct OAuthEntry {
  pub name: String,
  pub display_name: String,
  pub provider: Box<dyn OAuthProvider + Send + Sync>,
  pub client: OAuthClient,
}

pub struct AuthOptions {
  password_options: PasswordOptions,
  /// List of OAuth providers by `name`.
  oauth_providers: Vec<OAuthEntry>,
}

impl AuthOptions {
  pub fn from_config(site_url: Option<&str>, config: AuthConfig) -> Self {
    return Self {
      password_options: PasswordOptions {
        min_length: config.password_minimal_length.unwrap_or(8) as usize,
        max_length: 128,
        must_contain_upper_and_lower_case: config
          .password_must_contain_upper_and_lower_case
          .unwrap_or(false),
        must_contain_digits: config.password_must_contain_digits.unwrap_or(false),
        must_contain_special_characters: config
          .password_must_contain_special_characters
          .unwrap_or(false),
      },
      oauth_providers: build_oauth_providers_from_config(site_url, config),
    };
  }

  pub fn password_options(&self) -> &PasswordOptions {
    return &self.password_options;
  }

  /// Looks up an OAuthEntry: provider + OAuth client for the given name.
  ///
  /// Note for all practical N of OAuth providers, a sweep will be cheaper than a table lookup.
  pub fn lookup_oauth_provider(&self, name: &str) -> Option<&OAuthEntry> {
    if let Some(entry) = self.oauth_providers.iter().find(|e| e.name == name) {
      return Some(entry);
    }
    return None;
  }

  /// Returns list of configured OAuth providers;
  pub fn list_oauth_providers(&self) -> &Vec<OAuthEntry> {
    return &self.oauth_providers;
  }
}

fn build_oauth_providers_from_config(
  site_url: Option<&str>,
  config: AuthConfig,
) -> Vec<OAuthEntry> {
  if config.oauth_providers.is_empty() {
    return vec![];
  }

  let errors = parking_lot::Mutex::new(Vec::<String>::new());
  defer! {
      let errors = errors.lock();
      if errors.is_empty() {
          return;
      }

      log::error!("Encountered errors during OAuth initialization:\n\t{}", errors.join("\n\t"));

      #[cfg(debug_assertions)]
      panic!("Fail on OAuth errors in debug builds.");
  }

  let mut errors = errors.lock();
  let Some(site_url) = site_url else {
    errors.push("Missing config.server.site_url. OAuth sign-in not possible".into());
    return vec![];
  };

  let site = match Url::parse(site_url) {
    Ok(site) => site,
    Err(err) => {
      errors.push(format!(
        "Failed to parse site_url. OAuth sign-in not possible: {err}"
      ));
      return vec![];
    }
  };

  let registry = oauth_providers_static_registry();

  let mut providers: Vec<OAuthEntry> = vec![];
  for (key, config) in &config.oauth_providers {
    let entry = registry
      .iter()
      .find(|registered| config.provider_id == Some(registered.id as i32));

    let Some(entry) = entry else {
      errors.push(format!("missing implementation for oauth provider: {key}"));
      continue;
    };

    let provider = match (entry.factory)(key, config) {
      Ok(provider) => provider,
      Err(err) => {
        errors.push(format!("failed to build OAuth provider: {err}"));
        continue;
      }
    };

    let client = match build_oauth_client(&site, provider.as_ref()) {
      Ok(client) => client,
      Err(err) => {
        errors.push(format!("failed to build OAuth client: {err}"));
        continue;
      }
    };

    providers.push(OAuthEntry {
      name: provider.name().to_string(),
      display_name: provider.display_name().to_string(),
      provider,
      client,
    })
  }

  // Question: should we preserve the config order instead of sorting by name?
  providers.sort_by(|a, b| Ord::cmp(&a.name, &b.name));

  return providers;
}

fn build_oauth_client(
  site: &Url,
  provider: &(dyn OAuthProvider + Send + Sync),
) -> Result<OAuthClient, AuthError> {
  use crate::constants::AUTH_API_PATH;
  use oauth2::{AuthUrl, Client, ClientId, ClientSecret, RedirectUrl, TokenUrl};

  let name = provider.name();
  let redirect_url: Url = site
    .join(&format!("/{AUTH_API_PATH}/oauth/{name}/callback",))
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  let settings = provider.settings()?;
  if settings.client_id.is_empty() {
    return Err(AuthError::Internal(
      format!("Missing client id for {name}").into(),
    ));
  }
  if settings.client_secret.is_empty() {
    return Err(AuthError::Internal(
      format!("Missing client secret for {name}").into(),
    ));
  }

  return Ok(
    Client::new(ClientId::new(settings.client_id))
      .set_client_secret(ClientSecret::new(settings.client_secret))
      .set_auth_uri(AuthUrl::from_url(settings.auth_url))
      .set_token_uri(TokenUrl::from_url(settings.token_url))
      .set_redirect_uri(RedirectUrl::from_url(redirect_url))
      .set_auth_type(provider.auth_type()),
  );
}
