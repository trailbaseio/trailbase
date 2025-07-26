use base64::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::IntoParams;

use crate::AppState;
use crate::auth::error::AuthError;
use crate::auth::util::validate_redirect;

#[derive(Clone, Debug, Default, Deserialize, Serialize, IntoParams, PartialEq)]
pub(crate) struct LoginInputParams {
  pub redirect_to: Option<String>,
  pub response_type: Option<String>,
  pub pkce_code_challenge: Option<String>,
}

impl LoginInputParams {
  pub(crate) fn merge(mut self, other: LoginInputParams) -> LoginInputParams {
    if let Some(redirect_to) = other.redirect_to {
      self.redirect_to.get_or_insert(redirect_to);
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
  Password {
    redirect_to: Option<String>,
  },
  ProofKeyForCodeExchange {
    redirect_to: String,
    pkce_code_challenge: String,
  },
}

pub(crate) fn build_and_validate_input_params(
  state: &AppState,
  params: LoginInputParams,
) -> Result<LoginParams, AuthError> {
  return match params.response_type.as_deref() {
    Some("code") => {
      let Some(redirect_to) = params.redirect_to else {
        return Err(AuthError::BadRequest("missing 'redirect_to'"));
      };

      validate_redirect(state, Some(&redirect_to))?;

      let Some(pkce_code_challenge) = params.pkce_code_challenge else {
        return Err(AuthError::BadRequest("missing 'pkce_code_challenge'"));
      };

      // QUESTION: Should we validate more, .e.g. length?
      let _ = BASE64_URL_SAFE_NO_PAD
        .decode(&pkce_code_challenge)
        .map_err(|_| AuthError::BadRequest("invalid 'pkce_code_challenge'"))?;

      Ok(LoginParams::ProofKeyForCodeExchange {
        redirect_to,
        pkce_code_challenge,
      })
    }
    Some(_) => Err(AuthError::BadRequest("invalid 'response_type'")),
    None => {
      if params.pkce_code_challenge.is_some() {
        return Err(AuthError::BadRequest(
          "set 'response_type=code' or remove pkce challenge",
        ));
      }

      validate_redirect(state, params.redirect_to.as_deref())?;

      Ok(LoginParams::Password {
        redirect_to: params.redirect_to,
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
      LoginParams::ProofKeyForCodeExchange {
        redirect_to: "/redirect".to_string(),
        pkce_code_challenge: BASE64_URL_SAFE.encode("challenge"),
      },
      build_and_validate_input_params(
        &state,
        LoginInputParams {
          redirect_to: Some("/redirect".to_string()),
          response_type: Some("code".to_string()),
          pkce_code_challenge: Some(BASE64_URL_SAFE.encode("challenge")),
        },
      )
      .unwrap()
    );

    assert_eq!(
      LoginParams::Password {
        redirect_to: Some("/redirect".to_string()),
      },
      build_and_validate_input_params(
        &state,
        LoginInputParams {
          redirect_to: Some("/redirect".to_string()),
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
          redirect_to: Some("invalid".to_string()),
          response_type: None,
          pkce_code_challenge: None,
        },
      )
      .is_err()
    );
  }
}
