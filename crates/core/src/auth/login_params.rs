use base64::prelude::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::auth::error::AuthError;
use crate::auth::util::validate_redirect;

/// https://www.rfc-editor.org/rfc/rfc6749#section-3.1.1
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, ToSchema, TS)]
pub enum ResponseType {
  /// Respond directly with auth token.
  #[serde(rename = "token")]
  Token,
  /// Respond with authorization code.
  #[serde(rename = "code")]
  Code,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, IntoParams, PartialEq)]
pub(crate) struct LoginInputParams {
  pub redirect_uri: Option<String>,
  pub totp_redirect_uri: Option<String>,
  pub response_type: Option<ResponseType>,
  pub pkce_code_challenge: Option<String>,
}

impl LoginInputParams {
  pub(crate) fn merge(mut self, other: LoginInputParams) -> LoginInputParams {
    if let Some(redirect_uri) = other.redirect_uri {
      self.redirect_uri.get_or_insert(redirect_uri);
    }
    if let Some(totp_redirect_uri) = other.totp_redirect_uri {
      self.totp_redirect_uri.get_or_insert(totp_redirect_uri);
    }
    if let Some(response_type) = other.response_type {
      self.response_type.get_or_insert(response_type);
    }
    if let Some(pkce_code_challenge) = other.pkce_code_challenge {
      self.pkce_code_challenge.get_or_insert(pkce_code_challenge);
    }
    return self;
  }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LoginParams {
  /// Access token flow.
  Password {
    redirect_uri: Option<String>,
    totp_redirect_uri: Option<String>,
  },
  /// Authorization code flow.
  AuthorizationCodeFlowWithPkce {
    redirect_uri: String,
    totp_redirect_uri: Option<String>,
    pkce_code_challenge: String,
  },
}

pub(crate) fn build_and_validate_input_params(
  state: &AppState,
  params: LoginInputParams,
) -> Result<LoginParams, AuthError> {
  validate_redirect(state, params.redirect_uri.as_deref())?;
  validate_redirect(state, params.totp_redirect_uri.as_deref())?;

  return match params.response_type.as_ref() {
    Some(ResponseType::Code) => {
      let redirect_uri = params
        .redirect_uri
        .ok_or_else(|| AuthError::BadRequest("missing 'redirect_uri'"))?;

      let pkce_code_challenge = params
        .pkce_code_challenge
        .ok_or_else(|| AuthError::BadRequest("missing 'pkce_code_challenge'"))?;

      // QUESTION: Should we validate more, .e.g. length?
      let _ = BASE64_URL_SAFE_NO_PAD
        .decode(&pkce_code_challenge)
        .map_err(|_| AuthError::BadRequest("invalid 'pkce_code_challenge'"))?;

      Ok(LoginParams::AuthorizationCodeFlowWithPkce {
        redirect_uri,
        totp_redirect_uri: params.totp_redirect_uri,
        pkce_code_challenge,
      })
    }
    Some(ResponseType::Token) | None => {
      if params.pkce_code_challenge.is_some() {
        return Err(AuthError::BadRequest(
          "set 'response_type=code' or remove pkce challenge",
        ));
      }

      Ok(LoginParams::Password {
        redirect_uri: params.redirect_uri,
        totp_redirect_uri: params.totp_redirect_uri,
      })
    }
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;

  #[tokio::test]
  async fn test_login_params_building() {
    let state = test_state(None).await.unwrap();

    assert_eq!(
      LoginParams::AuthorizationCodeFlowWithPkce {
        redirect_uri: "/redirect".to_string(),
        totp_redirect_uri: Some("/totp".to_string()),
        pkce_code_challenge: BASE64_URL_SAFE.encode("challenge"),
      },
      build_and_validate_input_params(
        &state,
        LoginInputParams {
          redirect_uri: Some("/redirect".to_string()),
          totp_redirect_uri: Some("/totp".to_string()),
          response_type: Some(ResponseType::Code),
          pkce_code_challenge: Some(BASE64_URL_SAFE.encode("challenge")),
        },
      )
      .unwrap()
    );

    assert_eq!(
      LoginParams::Password {
        redirect_uri: Some("/redirect".to_string()),
        totp_redirect_uri: None,
      },
      build_and_validate_input_params(
        &state,
        LoginInputParams {
          redirect_uri: Some("/redirect".to_string()),
          totp_redirect_uri: None,
          response_type: None,
          pkce_code_challenge: None,
        },
      )
      .unwrap()
    );

    assert!(
      build_and_validate_input_params(
        &state,
        LoginInputParams {
          redirect_uri: Some("invalid".to_string()),

          totp_redirect_uri: None,
          response_type: None,
          pkce_code_challenge: None,
        },
      )
      .is_err()
    );
  }
}
