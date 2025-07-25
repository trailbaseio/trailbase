use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) enum ResponseType {
  #[serde(rename = "code")]
  Code,
}

/// State that will be round-tripped from login -> remote oauth -> callback via the user's cookies.
///
/// NOTE: Consider encrypting the state to make it tamper-proof.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct OAuthState {
  /// Expiration timestamp. Required for JWT. We could remove this is we made this tamper-proof w/o
  /// JWT.
  pub exp: i64,

  /// OAuth CSRF protection. Needs to callback request.
  #[serde(rename = "secret")]
  pub csrf_secret: String,

  /// Server-side generated PKCE code verifier.
  ///
  /// The challenge is handed to the OAuth provider, so that the callback
  /// handler can send "auth-code+verifier" in return for an OAuth token.
  #[serde(rename = "verifier")]
  pub pkce_code_verifier: String,

  /// User-provided PKCE code challenge.
  ///
  /// The challenge is handed to us by the user. The verifier only lives on the client and is
  /// handed to us later on in /api/auth/v1/token. Importantly, this challenge is completely
  /// independent from the verifier above.
  #[serde(rename = "challenge")]
  pub user_pkce_code_challenge: Option<String>,

  /// If response type is "code", TrailBase will respond with an auth code rather than a token.
  ///
  /// user can subsequently convert the code with the PKCE verifier to an auth token using the
  /// token endpoint.
  #[serde(rename = "type")]
  pub response_type: Option<ResponseType>,

  /// Redirect target.
  pub redirect_to: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;

  #[tokio::test]
  async fn test_oauth_state_serialization() {
    let state = test_state(None).await.unwrap();

    let oauth_state = OAuthState {
      exp: chrono::Utc::now().timestamp() + 3600,
      csrf_secret: "secret".to_string(),
      pkce_code_verifier: "server verifier".to_string(),
      user_pkce_code_challenge: Some("client challenge".to_string()),
      response_type: Some(ResponseType::Code),
      redirect_to: Some("custom-sheme://test".to_string()),
    };

    let encoded = state.jwt().encode(&oauth_state).unwrap();
    let decoded: OAuthState = state.jwt().decode(&encoded).unwrap();

    assert_eq!(oauth_state, decoded);

    let serde_json::Value::Object(obj) = serde_json::to_value(&oauth_state).unwrap() else {
      panic!("expected obj");
    };
    assert_eq!("code", obj.get("type").unwrap().as_str().unwrap());
  }
}
