use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, HeaderName, Request};
use std::net::{IpAddr, SocketAddr};
use tower_governor::GovernorError;
use tower_governor::key_extractor::KeyExtractor;

use crate::config::proto;

fn permissive_extract_ip<T>(req: &Request<T>) -> Option<IpAddr> {
  let headers = req.headers();

  // NOTE: This code is mimicking axum_client_ip's pre v1 `InsecureClientIp::from`:
  return rightmost_x_forwarded_for(headers)
    .or_else(|_| client_ip::x_real_ip(headers))
    .or_else(|_| client_ip::fly_client_ip(headers))
    .or_else(|_| client_ip::true_client_ip(headers))
    .or_else(|_| client_ip::cf_connecting_ip(headers))
    .or_else(|_| client_ip::cloudfront_viewer_address(headers))
    .ok()
    .or_else(|| extract_peer_ip(req));
}

#[inline]
fn extract_peer_ip<T>(req: &Request<T>) -> Option<IpAddr> {
  let ext = req.extensions().get::<ConnectInfo<SocketAddr>>();

  // NOTE: For UDS, this would be ConnectInfo<tokio::net::unix::SocketAddr>, in which case we
  // cannot extract an `IpAddr` but we should expect the upstream proxy (that connects via UDS)
  // to attach a `X-Forwarded-For` or `X-Real-Ip` header, thus never making it here.
  #[cfg(all(debug_assertions, unix))]
  assert!(
    ext.is_some()
      || req
        .extensions()
        .get::<ConnectInfo<tokio::net::unix::SocketAddr>>()
        .is_some()
  );

  return ext.map(|ConnectInfo(addr)| addr.ip());
}

/// Extracts the rightmost IP address from the comma-separated list in the value of the last
/// `X-Forwarded-For` header.
pub fn rightmost_x_forwarded_for(header_map: &HeaderMap) -> Result<IpAddr, client_ip::Error> {
  let header_value = value_of_last_header(header_map, &X_FORWARDED_FOR)?;
  let first =
    header_value
      .rsplit(',')
      .next()
      .ok_or_else(|| client_ip::Error::MalformedHeaderValue {
        header_name: X_FORWARDED_FOR,
        header_value: header_value.to_owned(),
      })?;

  return first
    .trim()
    .parse::<IpAddr>()
    .map_err(|_| client_ip::Error::MalformedHeaderValue {
      header_name: X_FORWARDED_FOR,
      header_value: header_value.to_owned(),
    });
}

/// Extracts the rightmost "trusted" IP address from the comma-separated list in the
/// `X-Forwarded-For` header by walking right-to-left and removing trusted proxies.
pub fn rightmost_trusted_x_forwarded_for(
  header_map: &HeaderMap,
  trusted: &[IpAddr],
) -> Result<IpAddr, client_ip::Error> {
  let header_value = value_of_last_header(header_map, &X_FORWARDED_FOR)?;

  for candidate in header_value.rsplit(',') {
    let candidate_ip =
      candidate
        .trim()
        .parse::<IpAddr>()
        .map_err(|_| client_ip::Error::MalformedHeaderValue {
          header_name: X_FORWARDED_FOR,
          header_value: header_value.to_owned(),
        })?;

    if !trusted.contains(&candidate_ip) {
      return Ok(candidate_ip);
    }
  }

  return Err(client_ip::Error::MalformedHeaderValue {
    header_name: X_FORWARDED_FOR,
    header_value: header_value.to_owned(),
  });
}

/// Returns a value of the last occurring header.
#[inline]
fn value_of_last_header<'a>(
  header_map: &'a HeaderMap,
  header_name: &HeaderName,
) -> Result<&'a str, client_ip::Error> {
  return header_map
    .get_all(header_name)
    .into_iter()
    .next_back()
    .ok_or_else(|| client_ip::Error::AbsentHeader {
      header_name: header_name.to_owned(),
    })?
    .to_str()
    .map_err(|_| client_ip::Error::NonAsciiHeaderValue {
      header_name: header_name.to_owned(),
    });
}

#[derive(Debug, Clone)]
pub struct IpExtractor {
  header: proto::ProxyClientIpHeader,
  trusted_proxies: Vec<IpAddr>,
}

impl IpExtractor {
  pub fn new(config: Option<&proto::ProxyConfig>) -> Self {
    return Self {
      header: config.map_or(proto::ProxyClientIpHeader::Undefined, |c| {
        c.client_ip_header()
      }),
      trusted_proxies: config
        .map(|c| {
          c.trusted_proxies
            .iter()
            .filter_map(|ip| {
              return ip
                .trim()
                .parse::<IpAddr>()
                .map_err(|err| {
                  log::error!("ConfigError: failed to parse proxy ip '{ip}': {err}");
                  return err;
                })
                .ok();
            })
            .collect()
        })
        .unwrap_or_default(),
    };
  }

  pub fn extract_ip<T>(&self, req: &Request<T>) -> Option<IpAddr> {
    use proto::ProxyClientIpHeader;

    return match self.header {
      ProxyClientIpHeader::Undefined => permissive_extract_ip(req),
      ProxyClientIpHeader::None => extract_peer_ip(req),
      ProxyClientIpHeader::XForwardedFor if self.trusted_proxies.is_empty() => {
        rightmost_x_forwarded_for(req.headers())
          .ok()
          .or_else(|| extract_peer_ip(req))
      }
      ProxyClientIpHeader::XForwardedFor => {
        rightmost_trusted_x_forwarded_for(req.headers(), &self.trusted_proxies)
          .ok()
          .or_else(|| extract_peer_ip(req))
      }
      ProxyClientIpHeader::XRealIp => client_ip::x_real_ip(req.headers())
        .ok()
        .or_else(|| extract_peer_ip(req)),
      ProxyClientIpHeader::TrueClientIp => client_ip::true_client_ip(req.headers())
        .ok()
        .or_else(|| extract_peer_ip(req)),
      ProxyClientIpHeader::CfConnectingIp => client_ip::cf_connecting_ip(req.headers())
        .ok()
        .or_else(|| extract_peer_ip(req)),
      ProxyClientIpHeader::FlyClientIp => client_ip::fly_client_ip(req.headers())
        .ok()
        .or_else(|| extract_peer_ip(req)),

      ProxyClientIpHeader::CloudfrontViewerAddress => {
        client_ip::cloudfront_viewer_address(req.headers())
          .ok()
          .or_else(|| extract_peer_ip(req))
      }
    };
  }
}

impl KeyExtractor for IpExtractor {
  type Key = IpAddr;

  fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
    return self
      .extract_ip(req)
      .ok_or_else(|| GovernorError::UnableToExtractKey);
  }
}

const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

// fn ipv6_privacy_mask(ip: IpAddr) -> IpAddr {
//   return match ip {
//     IpAddr::V4(ip) => IpAddr::V4(ip),
//     IpAddr::V6(ip) => IpAddr::V6(From::from(
//       ip.to_bits() & 0xFFFF_FFFF_FFFF_FFFF_0000_0000_0000_0000,
//     )),
//   };
// }

#[cfg(test)]
mod tests {
  use super::*;

  use axum::http::HeaderValue;

  #[test]
  fn extract_x_forwarded_for() {
    let mut headers = HeaderMap::new();
    headers.insert(
      X_FORWARDED_FOR,
      HeaderValue::from_static("192.168.0.1 , 127.0.0.1 "),
    );

    assert_eq!(
      "127.0.0.1".parse::<IpAddr>().unwrap(),
      rightmost_x_forwarded_for(&headers).unwrap()
    );

    assert_eq!(
      "127.0.0.1".parse::<IpAddr>().unwrap(),
      rightmost_trusted_x_forwarded_for(&headers, &[]).unwrap()
    );
    assert_eq!(
      "127.0.0.1".parse::<IpAddr>().unwrap(),
      rightmost_trusted_x_forwarded_for(&headers, &["192.168.0.1".parse::<IpAddr>().unwrap()])
        .unwrap()
    );
    assert_eq!(
      "192.168.0.1".parse::<IpAddr>().unwrap(),
      rightmost_trusted_x_forwarded_for(&headers, &["127.0.0.1".parse::<IpAddr>().unwrap()])
        .unwrap()
    );
  }

  #[test]
  fn ip_extractor() {
    let config = proto::ProxyConfig {
      client_ip_header: Some(proto::ProxyClientIpHeader::XForwardedFor as i32),
      trusted_proxies: vec!["127.0.0.1".to_string()],
    };

    let extractor = IpExtractor::new(Some(&config));

    let request = Request::builder()
      .header(
        X_FORWARDED_FOR,
        HeaderValue::from_static("192.168.0.1 , 127.0.0.1 "),
      )
      .body(axum::body::Body::empty())
      .unwrap();

    assert_eq!(
      "192.168.0.1".parse::<IpAddr>().unwrap(),
      extractor.extract_ip(&request).unwrap()
    );
  }
}
