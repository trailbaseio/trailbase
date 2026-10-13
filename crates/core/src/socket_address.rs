use std::net::ToSocketAddrs;

#[derive(Clone, Debug, PartialEq)]
pub enum SocketAddr {
  /// A Tcp socket address.
  Tcp(std::net::SocketAddr),
  /// UDS socket address.
  Uds(std::path::PathBuf),
}

impl SocketAddr {
  pub fn parse(address: &str) -> Result<Self, String> {
    let address = address.trim();
    if let Some((_, uds)) = address.split_once("unix:") {
      return Ok(SocketAddr::Uds(uds.into()));
    }

    fn parse_socket_addr(address: &str) -> Result<SocketAddr, String> {
      if let Ok(mut iter) = address.to_socket_addrs()
        && let Some(a) = iter.next()
      {
        return Ok(SocketAddr::Tcp(a));
      }

      return Err(format!("failed to parse socket address: {address}"));
    }

    if let Ok(a) = parse_socket_addr(address) {
      return Ok(a);
    }

    return parse_socket_addr(&format!("{address}:4000"));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_parse_bind_address() {
    assert_eq!(
      SocketAddr::Uds("/test".into()),
      SocketAddr::parse("unix:/test").unwrap()
    );

    SocketAddr::parse("0.0.0.0:4000").unwrap();
    SocketAddr::parse("localhost:4000").unwrap();
  }
}
