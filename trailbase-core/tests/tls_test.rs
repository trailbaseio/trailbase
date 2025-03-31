use rcgen::{generate_simple_self_signed, CertifiedKey};
use tokio_rustls::rustls::pki_types::{pem::PemObject, PrivateKeyDer};
use tracing::*;
use trailbase::{DataDir, Server, ServerOptions};

#[test]
fn test_https_serving() {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let data_dir = temp_dir::TempDir::new().unwrap();

  // Generate a certificate valid for "trailbase.io" and "localhost".
  let subject_alt_names = vec!["trailbase.io".to_string(), "localhost".to_string()];

  let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names).unwrap();

  let _ = runtime.block_on(async move {
    let port = 4025;
    let address = format!("127.0.0.1:{port}");

    let tls_pem = key_pair.serialize_pem();
    let tls_key = PrivateKeyDer::from_pem_slice(tls_pem.as_bytes()).unwrap();

    let app = Server::init(ServerOptions {
      data_dir: DataDir(data_dir.path().to_path_buf()),
      address: address.to_string(),
      admin_address: None,
      public_dir: None,
      dev: false,
      disable_auth_ui: false,
      cors_allowed_origins: vec![],
      js_runtime_threads: None,
      tls_key: Some(tls_key),
      tls_cert: Some(cert.der().clone()),
      ..Default::default()
    })
    .await
    .unwrap();

    let _server = tokio::spawn(async move {
      app.serve().await.unwrap();
    });

    let client = reqwest::ClientBuilder::new()
      .add_root_certificate(reqwest::Certificate::from_pem(cert.pem().as_bytes()).unwrap())
      .use_rustls_tls()
      .min_tls_version(reqwest::tls::Version::TLS_1_3)
      .build()
      .unwrap();

    'success: {
      for _ in 0..100 {
        let response = client
          .get(&format!("https://localhost:{port}/api/healthcheck"))
          .send()
          .await;

        debug!("{response:?}");

        if let Ok(response) = response {
          assert_eq!(response.text().await.unwrap(), "Ok");
          break 'success;
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
      }

      panic!("Timed out");
    }
  });
}
