use axum::http::StatusCode;
use axum_test::TestServer;
use std::io::Write;

use trailbase::{DataDir, Server, ServerOptions};

#[tokio::test]
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

  // Create docs directory with index.html (for directory index test)
  let docs_dir = public_dir.path().join("docs");
  std::fs::create_dir(&docs_dir).unwrap();
  let docs_index_path = docs_dir.join("index.html");
  let mut docs_index_file = std::fs::File::create(&docs_index_path).unwrap();
  docs_index_file
    .write_all("<!DOCTYPE html><html><body>Docs Index</body></html>".as_bytes())
    .unwrap();

  // Test SPA mode enabled - "most" non-existent routes return index.html
  let options = ServerOptions {
    data_dir: DataDir(data_dir.path().to_path_buf()),
    address: "localhost:4051".to_string(),
    admin_address: None,
    public_dir: Some(public_dir.path().to_path_buf()),
    public_dir_spa: true,
    dev: false,
    cors_allowed_origins: vec![],
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

  // Vite-style .html resolution: /about should serve index.html.
  let response = server.get("/about").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(
    response.text().contains("SPA Index"),
    "Expected about.html content for /about, got: {}",
    response.text()
  );

  // Non-existent route without matching .html should return index.html
  let response = server.get("/contact").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(response.text().contains("SPA Index"));

  // Directory with trailing slash should serve its index.html
  let response = server.get("/docs/").await;
  assert_eq!(response.status_code(), StatusCode::OK);
  assert!(
    response.text().contains("Docs Index"),
    "Expected docs/index.html content for /docs/, got: {}",
    response.text()
  );

  // Route with dot in path segment (not extension) should return index.html
  let response = server.get("/user.name/profile").await;
  assert_eq!(
    response.status_code(),
    StatusCode::OK,
    "Expected OK for route /user.name/profile with dot in path"
  );
  assert!(
    response.text().contains("SPA Index"),
    "Expected SPA index for /user.name/profile"
  );

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

  // Non-existent other file should return 404
  let response = server.get("/assets/missing.html").await;
  assert_eq!(response.status_code(), StatusCode::NOT_FOUND);

  // ServeDir behavior: redirect
  let response = server.get("/docs").await;
  assert_eq!(response.status_code(), StatusCode::TEMPORARY_REDIRECT);
  assert_eq!(response.header("location"), "http://localhost/docs/");
}
