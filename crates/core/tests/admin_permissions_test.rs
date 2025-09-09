use axum::http::StatusCode;
use axum_test::TestServer;

use trailbase::{DataDir, Server, ServerOptions};

#[test]
fn test_admin_permissions() {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let data_dir = temp_dir::TempDir::new().unwrap();

  let _ = runtime.block_on(async move {
    let Server {
      state: _,
      main_router,
      admin_router,
      tls,
    } = Server::init(ServerOptions {
      data_dir: DataDir(data_dir.path().to_path_buf()),
      address: "localhost:4040".to_string(),
      admin_address: None,
      public_dir: None,
      dev: false,
      disable_auth_ui: false,
      cors_allowed_origins: vec![],
      runtime_threads: None,
      ..Default::default()
    })
    .await
    .unwrap();

    assert!(admin_router.is_none());
    assert!(tls.is_none());

    let server = TestServer::new(main_router.1).unwrap();

    assert_eq!(
      server.get("/api/healthcheck").await.status_code(),
      StatusCode::OK
    );

    assert_eq!(
      server.get("/api/_admin/tables").await.status_code(),
      StatusCode::UNAUTHORIZED
    );
  });
}
