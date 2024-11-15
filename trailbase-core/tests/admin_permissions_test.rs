use axum::http::StatusCode;
use axum_test::TestServer;
use std::rc::Rc;

use trailbase_core::{DataDir, Server, ServerOptions};

#[test]
fn test_admin_permissions() {
  let runtime = Rc::new(
    tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()
      .unwrap(),
  );

  let data_dir = temp_dir::TempDir::new().unwrap();

  let _ = runtime.clone().block_on(async move {
    let app = Server::init(ServerOptions {
      data_dir: DataDir(data_dir.path().to_path_buf()),
      address: "".to_string(),
      admin_address: None,
      public_dir: None,
      dev: false,
      disable_auth_ui: false,
      cors_allowed_origins: vec![],
      tokio_runtime: runtime,
      js_runtime_threads: None,
    })
    .await
    .unwrap();
    let server = TestServer::new(app.router().clone()).unwrap();

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
