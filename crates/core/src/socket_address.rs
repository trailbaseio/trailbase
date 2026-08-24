use std::net::ToSocketAddrs;

#[derive(Clone, Debug, PartialEq)]
pub enum SocketAddr {
  /// A Tcp socket address.
  Tcp(std::net::SocketAddr),
  /// UDS socket address.
  Uds(std::path::PathBuf),
}

impl SocketAddr {
  pub fn parse(address: &str) -> Result<Self, std::io::Error> {
    let address = address.trim();
    if let Some((_, uds)) = address.split_once("unix:") {
      return Ok(SocketAddr::Uds(uds.into()));
    }

    if let Some(a) = address.to_socket_addrs()?.next() {
      return Ok(SocketAddr::Tcp(a));
    }

    return Err(std::io::Error::other(format!(
      "failed to parse socket address: {address}"
    )));
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
