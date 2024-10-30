use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub(crate) enum ResponseType {
  Code,
}

/// State that will be round-tripped from login -> remote oauth -> callback via the user's cookies.
///
/// NOTE: Consider encrypting the state to make it tamper proof.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct OAuthState {
  /// Expiration timestamp. Required for JWT.
  pub exp: i64,

  /// OAuth CSRF protection. Needs to callback request.
  #[serde(alias = "secret")]
  pub csrf_secret: String,

  /// Server-side generated PKCE code verifier.
  ///
  /// The challenge is handed to the OAuth provider, so that the callback
  /// handler can send "auth-code+verifier" in return for an OAuth token.
  #[serde(alias = "verifier")]
  pub pkce_code_verifier: String,

  /// User-provided PKCE code challenge.
  ///
  /// The challenge is handed to us by the user. The verifier only lives on the
  /// client and is handed to us later on. Importantly, this challenge is
  /// completely independent from the verifier above.
  #[serde(alias = "challenge")]
  pub user_pkce_code_challenge: Option<String>,

  /// If response type is "code", TrailBase will respond with an auth code rather than a token.
  ///
  /// user can subsequently convert the code with the PKCE verifier to an auth token using the
  /// token endpoint.
  #[serde(alias = "type")]
  pub response_type: Option<ResponseType>,

  /// Redirect target.
  pub redirect_to: Option<String>,
}
