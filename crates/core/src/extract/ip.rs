use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::uri::Authority;
use axum::http::{Extensions, HeaderMap, HeaderValue, Request, request::Parts};
use std::convert::Infallible;
use std::net::IpAddr;
use tower_governor::GovernorError;
use tower_governor::key_extractor::KeyExtractor;

pub fn extract_ip(headers: &HeaderMap<HeaderValue>, ext: &Extensions) -> Option<std::net::IpAddr> {
  // NOTE: This code is mimicking axum_client_ip's pre v1 `InsecureClientIp::from`:
  return client_ip::rightmost_x_forwarded_for(headers)
    .or_else(|_| client_ip::x_real_ip(headers))
    .or_else(|_| client_ip::fly_client_ip(headers))
    .or_else(|_| client_ip::true_client_ip(headers))
    .or_else(|_| client_ip::cf_connecting_ip(headers))
    .or_else(|_| client_ip::cloudfront_viewer_address(headers))
    .ok()
    .or_else(|| {
      ext
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip())
    });
}

/// Key extractor for the Governor.
#[derive(Debug, Clone)]
pub struct RealIpKeyExtractor;

impl KeyExtractor for RealIpKeyExtractor {
  type Key = IpAddr;

  fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
    return extract_ip(req.headers(), req.extensions())
      .ok_or_else(|| GovernorError::UnableToExtractKey);
  }

  // fn name(&self) -> &'static str {
  //   "smart IP"
  // }

  // fn key_name(&self, key: &Self::Key) -> Option<String> {
  //   Some(key.to_string())
  // }
}

// RealIp extractor for handlers.
#[derive(Debug, Clone, Default)]
pub struct RealIp(pub Option<std::net::IpAddr>);

impl<S> FromRequestParts<S> for RealIp
where
  S: Send + Sync,
{
  type Rejection = Infallible;

  async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
    return Ok(Self(extract_ip(&parts.headers, &parts.extensions)));
  }
}

// Host extractor for handlers.
#[derive(Debug, Clone, Default)]
pub struct Host(pub Option<Authority>);

impl<S> FromRequestParts<S> for Host
where
  S: Send + Sync,
{
  type Rejection = Infallible;

  async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
    let header = parts
      .headers
      .get("X-Forwarded-Host")
      .or_else(|| parts.headers.get("host"));

    return Ok(Self(
      header.and_then(|h| Authority::try_from(h.as_bytes()).ok()),
    ));
  }
}

#[allow(unused)]
fn ipv6_privacy_mask(ip: IpAddr) -> IpAddr {
  return match ip {
    IpAddr::V4(ip) => IpAddr::V4(ip),
    IpAddr::V6(ip) => IpAddr::V6(From::from(
      ip.to_bits() & 0xFFFF_FFFF_FFFF_FFFF_0000_0000_0000_0000,
    )),
  };
}
