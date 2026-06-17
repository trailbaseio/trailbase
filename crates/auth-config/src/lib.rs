use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum LoginIdentifier {
  Email,
  Handle,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
  pub img_name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuthConfig {
  pub disable_password_auth: bool,
  pub enable_otp_signin: bool,
  pub oauth_providers: Vec<OAuthProvider>,
  pub login_identifier: Vec<LoginIdentifier>,
}
