use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{Method, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::{RequestExt, Router};
use http_body_util::BodyExt;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{ErrorData as McpError, ServerCapabilities, ServerInfo};
use rmcp::transport::{
  StreamableHttpServerConfig,
  streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use rmcp::{Json, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::Deserialize;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::admin;
use crate::app_state::AppState;
use crate::auth::util::is_admin;
use crate::auth::{AuthError, User};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AdminRequest {
  /// HTTP method accepted by the TrailBase admin API.
  method: String,
  /// Admin API path relative to /api/_admin, including an optional query string.
  path: String,
  /// Optional JSON request body.
  #[serde(default)]
  body: Option<Value>,
}

#[derive(Clone)]
struct TrailBaseMcp {
  state: AppState,
  #[allow(dead_code)]
  tool_router: ToolRouter<Self>,
}

impl TrailBaseMcp {
  fn new(state: AppState) -> Self {
    Self {
      state,
      tool_router: Self::tool_router(),
    }
  }
}

#[tool_router]
impl TrailBaseMcp {
  #[tool(
    description = "Call a TrailBase admin API in-process. Paths are relative to /api/_admin. This exposes the same table, index, row, config, schema, query, user, log, backup, job, and WASM operations as the admin dashboard."
  )]
  async fn call_admin_api(
    &self,
    Parameters(request): Parameters<AdminRequest>,
  ) -> Result<Json<Value>, McpError> {
    let method = Method::from_bytes(request.method.as_bytes())
      .map_err(|_| McpError::invalid_params("invalid HTTP method", None))?;
    let path = normalize_admin_path(&request.path)?;
    let body = request
      .body
      .map(|value| serde_json::to_vec(&value))
      .transpose()
      .map_err(|err| McpError::invalid_params(err.to_string(), None))?
      .unwrap_or_default();

    let request = Request::builder()
      .method(method)
      .uri(path)
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(body))
      .map_err(|err| McpError::internal_error(err.to_string(), None))?;

    let response = admin::router()
      .with_state(self.state.clone())
      .oneshot(request)
      .await
      .map_err(|never| match never {})?;
    let status = response.status();
    let bytes = response
      .into_body()
      .collect()
      .await
      .map_err(|err| McpError::internal_error(err.to_string(), None))?
      .to_bytes();

    let response_body = if bytes.is_empty() {
      Value::Null
    } else {
      serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };

    if !status.is_success() {
      return Err(McpError::internal_error(
        format!("TrailBase admin API returned {status}: {response_body}"),
        Some(json!({ "status": status.as_u16(), "body": response_body })),
      ));
    }

    Ok(Json(json!({
      "status": status.as_u16(),
      "body": response_body
    })))
  }
}

#[tool_handler]
impl ServerHandler for TrailBaseMcp {
  fn get_info(&self) -> ServerInfo {
    ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
      "TrailBase's native administrative MCP server. Use call_admin_api to perform the same operations as the admin dashboard. Destructive operations modify the active TrailBase depot.",
    )
  }
}

pub(crate) fn router(state: &AppState) -> Router<AppState> {
  let state_for_service = state.clone();
  let service: StreamableHttpService<TrailBaseMcp, LocalSessionManager> =
    StreamableHttpService::new(
      move || Ok(TrailBaseMcp::new(state_for_service.clone())),
      Arc::new(LocalSessionManager::default()),
      StreamableHttpServerConfig::default()
        .with_json_response(true)
        // TrailBase supports operator-configured reverse proxies. Admin authentication below is
        // the security boundary, so the MCP transport must accept the proxy's Host header.
        .disable_allowed_hosts(),
    );

  Router::new()
    .nest_service("/mcp", service)
    .layer(middleware::from_fn_with_state(
      state.clone(),
      assert_mcp_access,
    ))
}

async fn assert_mcp_access(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Result<Response, AuthError> {
  let user = request.extract_parts_with_state::<User, _>(&state).await?;
  if !is_admin(&state, &user.uuid).await {
    return Err(AuthError::Forbidden);
  }

  Ok(next.run(request).await)
}

fn normalize_admin_path(path: &str) -> Result<String, McpError> {
  let path = path.trim();
  if path.is_empty() || path.contains("://") || path.starts_with("//") {
    return Err(McpError::invalid_params("invalid admin API path", None));
  }

  let path = path
    .strip_prefix("/api/_admin")
    .unwrap_or(path)
    .trim_start_matches('/');
  Ok(format!("/{path}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalizes_admin_paths() {
    assert_eq!(normalize_admin_path("tables").unwrap(), "/tables");
    assert_eq!(
      normalize_admin_path("/api/_admin/logs/list?limit=5").unwrap(),
      "/logs/list?limit=5"
    );
    assert!(normalize_admin_path("https://example.com").is_err());
  }
}
