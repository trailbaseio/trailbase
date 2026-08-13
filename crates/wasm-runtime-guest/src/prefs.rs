use thiserror::Error;
use trailbase_wasm_common::{PrefsRequest, PrefsResponse};
use wstd::http::body::IntoBody;
use wstd::http::{Client, Request};

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum PrefsError {
  #[error("NotFound")]
  NotFound,
  #[error("Serialization: {0}")]
  Serialization(Box<dyn std::error::Error>),
  #[error("Unexpected Type: {0}")]
  UnexpectedType(Box<dyn std::error::Error>),
  #[error("Other: {0}")]
  Other(Box<dyn std::error::Error>),
}

impl From<serde_json::Error> for PrefsError {
  fn from(err: serde_json::Error) -> Self {
    return Self::Serialization(err.into());
  }
}

pub async fn get_prefs(key: &str) -> Result<Option<String>, PrefsError> {
  let r = PrefsRequest::Get {
    key: key.to_string(),
  };
  let request = Request::builder()
    .uri("http://__prefs")
    .method("POST")
    .body(serde_json::to_vec(&r)?.into_body())
    .map_err(|err| PrefsError::Other(err.into()))?;

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request)
    .await
    .map_err(|err| PrefsError::Other(err.into()))?
    .into_parts();

  let bytes = body
    .bytes()
    .await
    .map_err(|err| PrefsError::Other(err.into()))?;

  return match serde_json::from_slice(&bytes) {
    Ok(PrefsResponse::Value(value)) => Ok(value),
    Ok(PrefsResponse::Error(err)) => Err(PrefsError::Other(err.into())),
    Ok(resp) => Err(PrefsError::UnexpectedType(
      format!("Expected Value, got: {resp:?}").into(),
    )),
    Err(err) => Err(PrefsError::Other(err.into())),
  };
}

// TODO: When WASIp3 actually works ,we should probably have dedicated [set|get]_prefs endpoints
// and push the component name mapping responsibility into the host. Would also allow for cross
// request caching and invalidation.
pub async fn set_prefs(
  key: &str,
  value: Option<impl std::string::ToString>,
) -> Result<(), PrefsError> {
  let r = PrefsRequest::Set {
    key: key.to_string(),
    value: value.map(|v| v.to_string()),
  };
  let request = Request::builder()
    .uri("http://__prefs")
    .method("POST")
    .body(serde_json::to_vec(&r)?.into_body())
    .map_err(|err| PrefsError::Other(err.into()))?;

  let client = Client::new();
  let (_parts, mut body) = client
    .send(request)
    .await
    .map_err(|err| PrefsError::Other(err.into()))?
    .into_parts();

  let bytes = body
    .bytes()
    .await
    .map_err(|err| PrefsError::Other(err.into()))?;

  return match serde_json::from_slice(&bytes) {
    Ok(PrefsResponse::Ok) => Ok(()),
    Ok(PrefsResponse::Error(err)) => Err(PrefsError::Other(err.into())),
    Ok(resp) => Err(PrefsError::UnexpectedType(
      format!("Expected Ok or Error, got: {resp:?}").into(),
    )),
    Err(err) => Err(PrefsError::Other(err.into())),
  };
}
