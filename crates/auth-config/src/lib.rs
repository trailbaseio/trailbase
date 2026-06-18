use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum LoginIdentifier {
  OnlyEmail,
  OnlyHandle,
  EmailOrHandle,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub enum RegistrationIdentifier {
  OnlyEmail,
  RequireEmail,
  OnlyHandle,
  RequireHandle,
  EmailAndHandle,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
  pub img_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthConfig {
  /// Whether to allow login using passwords.
  pub disable_password_auth: bool,
  /// Whether to allow requesting OTP codes via email.
  pub enable_otp_signin: bool,
  /// List of OAuth providers.
  pub oauth_providers: Vec<OAuthProvider>,
  /// What user identifier can be used to log in.
  pub login_identifier: LoginIdentifier,
  /// What user identifiers need to be provided to register a new user.
  pub registration_identifier: RegistrationIdentifier,
}
