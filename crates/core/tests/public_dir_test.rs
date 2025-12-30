use axum::http::StatusCode;
use axum_test::TestServer;
use std::io::Write;

use trailbase::{DataDir, Server, ServerOptions};

#[tokio::test]
async fn test_without_spa_fallback() {
  let data_dir = temp_dir::TempDir::new().unwrap();
  let public_dir = temp_dir::TempDir::new().unwrap();

  // Create test files in public_dir
  let index_html_content = "<!DOCTYPE html><html><body>Static Index</body></html>";

  // Create index.html
  let index_path = public_dir.path().join("index.html");
  let mut index_file = std::fs::File::create(&index_path).unwrap();
  index_file.write_all(index_html_content.as_bytes()).unwrap();

  // Test SPA mode disabled - non-existent routes return 404
  let options = ServerOptions {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    address: "localhost:4052".to_string(),
    admin_address: None,
    public_dir: Some(public_dir.path().to_path_buf()),
    public_dir_spa: false,
    dev: false,
    cors_allowed_origins: vec![],
    runtime_threads: None,
    ..Default::default()
  };

  let Server { main_router, .. } = Server::init(options).await.unwrap();

  let (_address, router) = main_router;
  let server = TestServer::new(router).unwrap();

  // Existing file should be served
  let response = server.get("/index.html").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(response.text().contains("Static Index"));

  // Non-existent route should return 404 (not SPA fallback)
  let response = server.get("/user/profile").await;
  assert_eq!(
    response.status_code(),
    StatusCode::NOT_FOUND,
    "Expected 404 for non-existent route /user/profile without SPA mode, got {}",
    response.status_code()
  );
  assert_eq!(response.text(), "Not found");

  // Another non-existent route should return 404
  let response = server.get("/about").await;
  assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
  assert_eq!(response.text(), "Not found");

  // Deep nested route should return 404
  let response = server.get("/app/dashboard/settings").await;
  assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
  assert_eq!(response.text(), "Not found");

  // Non-existent file should also return 404
  let response = server.get("/favicon.ico").await;
  assert_eq!(
    response.status_code(),
    StatusCode::NOT_FOUND,
    "Expected 404 for non-existent file /favicon.ico"
  );
  assert_eq!(response.text(), "Not found");
}
