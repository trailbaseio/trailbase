use reqwest::Method;
use reqwest::header::HeaderMap;
use serde::de::DeserializeOwned;
use std::borrow::Cow;

use crate::Error;

#[async_trait::async_trait]
pub trait Transport {
  async fn fetch(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    body: Option<Vec<u8>>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<http::Response<reqwest::Body>, Error>;

  #[cfg(feature = "ws")]
  async fn upgrade_ws(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest_websocket::UpgradeResponse, Error>;
}

pub struct DefaultTransport {
  client: reqwest::Client,
  url: url::Url,
}

impl DefaultTransport {
  pub fn new(url: url::Url) -> Self {
    return Self {
      // QUESTION: Should we follow redirects?
      client: reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("like reqwest::Client::new()"),
      url,
    };
  }
}

#[async_trait::async_trait]
impl Transport for DefaultTransport {
  async fn fetch(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    body: Option<Vec<u8>>,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<http::Response<reqwest::Body>, Error> {
    assert!(path.starts_with("/"));

    let mut url = self.url.clone();
    url.set_path(path);

    if let Some(query_params) = query_params {
      let mut params = url.query_pairs_mut();
      for (key, value) in query_params {
        params.append_pair(key, value);
      }
    }

    let request = {
      let mut builder = self.client.request(method, url).headers(headers);
      if let Some(body) = body {
        // let json = serde_json::to_string(body).map_err(Error::RecordSerialization)?;
        // builder = builder.body(json);
        builder = builder.body(body);
      }
      builder.build()?
    };

    return Ok(self.client.execute(request).await?.into());
  }

  #[cfg(feature = "ws")]
  async fn upgrade_ws(
    &self,
    path: &str,
    headers: HeaderMap,
    method: Method,
    query_params: Option<&[(Cow<'static, str>, Cow<'static, str>)]>,
  ) -> Result<reqwest_websocket::UpgradeResponse, Error> {
    use reqwest_websocket::Upgrade;

    assert!(path.starts_with("/"));

    let mut url = self.url.clone();
    url.set_path(path);

    if let Some(query_params) = query_params {
      let mut params = url.query_pairs_mut();
      for (key, value) in query_params {
        params.append_pair(key, value);
      }
    }

    return Ok(
      self
        .client
        .request(method, url)
        .headers(headers)
        .upgrade()
        .send()
        .await?,
    );
  }
}

#[inline]
pub(crate) async fn json<T: DeserializeOwned>(
  resp: http::Response<reqwest::Body>,
) -> Result<T, Error> {
  let full = into_bytes(resp).await?;
  return serde_json::from_slice(&full).map_err(Error::RecordSerialization);
}

#[inline]
pub(crate) async fn into_bytes(resp: http::Response<reqwest::Body>) -> Result<bytes::Bytes, Error> {
  return Ok(
    http_body_util::BodyExt::collect(resp.into_body())
      .await
      .map(|buf| buf.to_bytes())?,
  );
}
