use axum::http::StatusCode;
use axum_test::TestServer;
use std::io::Write;

use trailbase::{DataDir, Server, ServerOptions};

#[test]
fn public_dir_spa_fallback_test() {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let _ = runtime.block_on(test_spa_fallback());
}

async fn test_spa_fallback() {
  let data_dir = temp_dir::TempDir::new().unwrap();
  let public_dir = temp_dir::TempDir::new().unwrap();

  // Create test files in public_dir
  let index_html_content = "<!DOCTYPE html><html><body>SPA Index</body></html>";
  let css_content = "body { color: red; }";

  // Create index.html
  let index_path = public_dir.path().join("index.html");
  let mut index_file = std::fs::File::create(&index_path).unwrap();
  index_file.write_all(index_html_content.as_bytes()).unwrap();

  // Create assets directory and style.css
  let assets_dir = public_dir.path().join("assets");
  std::fs::create_dir(&assets_dir).unwrap();
  let css_path = assets_dir.join("style.css");
  let mut css_file = std::fs::File::create(&css_path).unwrap();
  css_file.write_all(css_content.as_bytes()).unwrap();

  // Test SPA mode enabled - non-existent routes return index.html
  let options = ServerOptions {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    address: "localhost:4051".to_string(),
    admin_address: None,
    public_dir: Some(public_dir.path().to_path_buf()),
    public_dir_spa: true,
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
  assert!(response.text().contains("SPA Index"));

  // Existing CSS file should be served
  let response = server.get("/assets/style.css").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(response.text().contains("color: red"));

  // Non-existent route (no extension) should return index.html
  let response = server.get("/user/profile").await;
  assert_eq!(
    response.status_code(),
    StatusCode::OK,
    "Expected OK for SPA route /user/profile, got {}",
    response.status_code()
  );
  assert!(
    response.text().contains("SPA Index"),
    "Expected SPA index content for /user/profile"
  );

  // Another non-existent route should return index.html
  let response = server.get("/about").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(response.text().contains("SPA Index"));

  // Deep nested route should return index.html
  let response = server.get("/app/dashboard/settings").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(response.text().contains("SPA Index"));

  // Non-existent file (with extension) should return 404
  let response = server.get("/favicon.ico").await;
  assert_eq!(
    response.status_code(),
    StatusCode::NOT_FOUND,
    "Expected 404 for non-existent file /favicon.ico"
  );
  assert_eq!(response.text(), "Not found");

  // Non-existent CSS file should return 404
  let response = server.get("/assets/missing.css").await;
  assert_eq!(response.status_code(), StatusCode::NOT_FOUND);

  // Non-existent JS file should return 404
  let response = server.get("/bundle.js").await;
  assert_eq!(response.status_code(), StatusCode::NOT_FOUND);
}

#[test]
fn public_dir_without_spa_test() {
  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()
    .unwrap();

  let _ = runtime.block_on(test_without_spa_fallback());
}

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
