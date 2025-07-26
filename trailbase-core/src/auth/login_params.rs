use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::auth::error::AuthError;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, ToSchema)]
pub(crate) enum ResponseType {
  #[serde(rename = "code")]
  Code,
}

impl TryFrom<&str> for ResponseType {
  type Error = AuthError;

  fn try_from(value: &str) -> Result<Self, Self::Error> {
    return match value {
      "code" => Ok(ResponseType::Code),
      _ => Err(AuthError::BadRequest("invalid response_type")),
    };
  }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, IntoParams, PartialEq)]
pub(crate) struct LoginArguments {
  pub redirect_to: Option<String>,
  pub response_type: Option<ResponseType>,
  pub pkce_code_challenge: Option<String>,
}

pub(crate) trait LoginParams {
  fn redirect_to(&self) -> Option<&str>;
  fn resonse_type(&self) -> Option<ResponseType>;
  fn pkce_code_challenge(&self) -> Option<&str>;
}

impl LoginParams for LoginArguments {
  fn redirect_to(&self) -> Option<&str> {
    return self.redirect_to.as_deref();
  }
  fn resonse_type(&self) -> Option<ResponseType> {
    return self.response_type;
  }
  fn pkce_code_challenge(&self) -> Option<&str> {
    return self.pkce_code_challenge.as_deref();
  }
}

pub(crate) fn validate_login_params(params: &impl LoginParams) -> Result<(), AuthError> {
  return Ok(());
  // return Err(AuthError::BadRequest("invalid param".into()));
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_login_params_serialization() {
    let params = LoginArguments {
      redirect_to: Some("redirect".to_string()),
      response_type: Some(ResponseType::Code),
      pkce_code_challenge: Some("challenge".to_string()),
    };

    let serde_json::Value::Object(obj) = serde_json::to_value(&params).unwrap() else {
      panic!("Not an object");
    };

    assert_eq!("code", obj.get("response_type").unwrap().as_str().unwrap());

    let deserialized: LoginArguments =
      serde_json::from_value(serde_json::Value::Object(obj)).unwrap();

    assert_eq!(params, deserialized);
  }
}
