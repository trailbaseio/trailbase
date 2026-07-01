use axum::http::StatusCode;
use axum_test::TestServer;

use trailbase::{AppState, DataDir, InitArgs, Server, ServerOptions};

#[test]
fn test_admin_permissions() {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let data_dir = temp_dir::TempDir::new().unwrap();

  let _ = runtime.block_on(async move {
    let (_new, state) = AppState::init(InitArgs {
      data_dir: DataDir(data_dir.path().to_path_buf()),
      public_dir: None,
      dev: false,
      ..Default::default()
    })
    .await
    .unwrap();

    let Server {
      state: _,
      main_router,
      admin_router,
      tls,
    } = Server::init(
      state,
      ServerOptions {
        // data_dir: DataDir(data_dir.path().to_path_buf()),
        address: "localhost:4040".to_string(),
        admin_address: None,
        public_dir: None,
        cors_allowed_origins: vec![],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    assert!(admin_router.is_none());
    assert!(tls.is_none());

    let server = TestServer::new(main_router.1);

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
