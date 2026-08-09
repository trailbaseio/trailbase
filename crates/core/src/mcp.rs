use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Form, Json as AxumJson, Path, Query, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{RequestExt, Router};
use http_body_util::BodyExt;
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::{ErrorData as McpError, Implementation, ServerCapabilities, ServerInfo};
use rmcp::transport::{
  StreamableHttpServerConfig,
  streamable_http_server::{session::local::LocalSessionManager, tower::StreamableHttpService},
};
use rmcp::{Json, ServerHandler, schemars, tool, tool_handler, tool_router};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::admin;
use crate::app_state::AppState;
use crate::auth::util::is_admin;
use crate::auth::{AuthError, User};

const MCP_SCOPE: &str = "mcp";
const MCP_PATH: &str = "/mcp";

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

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SqlRequest {
  /// One or more SQLite statements. Schema-changing statements refresh TrailBase metadata.
  query: String,
  /// Optional configured attached database names.
  #[serde(default)]
  attached_databases: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfigUpdateRequest {
  /// Complete TrailBase config in protobuf text format, as returned by get_config.
  config: String,
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

  async fn dispatch_admin(&self, request: AdminRequest) -> Result<Json<Value>, McpError> {
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

#[tool_router]
impl TrailBaseMcp {
  #[tool(
    description = "Call a TrailBase admin API in-process. Paths are relative to /api/_admin. This exposes the same table, index, row, config, schema, query, user, log, backup, job, and WASM operations as the admin dashboard."
  )]
  async fn call_admin_api(
    &self,
    Parameters(request): Parameters<AdminRequest>,
  ) -> Result<Json<Value>, McpError> {
    self.dispatch_admin(request).await
  }

  #[tool(description = "List TrailBase tables, views, columns, indexes, triggers, and metadata.")]
  async fn list_tables(&self) -> Result<Json<Value>, McpError> {
    self
      .dispatch_admin(AdminRequest {
        method: "GET".to_string(),
        path: "tables".to_string(),
        body: None,
      })
      .await
  }

  #[tool(
    description = "Execute SQL using TrailBase's admin query handler. Supports reads and writes; schema changes refresh cached metadata."
  )]
  async fn execute_sql(
    &self,
    Parameters(request): Parameters<SqlRequest>,
  ) -> Result<Json<Value>, McpError> {
    self
      .dispatch_admin(AdminRequest {
        method: "POST".to_string(),
        path: "query".to_string(),
        body: Some(json!({
          "query": request.query,
          "attached_databases": request.attached_databases,
        })),
      })
      .await
  }

  #[tool(
    description = "Get the complete TrailBase configuration as protobuf text. Secret values are redacted."
  )]
  fn get_config(&self) -> Result<String, McpError> {
    let (config, _) = crate::config::redact_secrets(&self.state.get_config())
      .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    config
      .to_text()
      .map_err(|err| McpError::internal_error(err.to_string(), None))
  }

  #[tool(
    description = "Validate and replace the TrailBase configuration using protobuf text from get_config. Existing secret values are preserved."
  )]
  async fn update_config(
    &self,
    Parameters(request): Parameters<ConfigUpdateRequest>,
  ) -> Result<String, McpError> {
    if self.state.demo_mode() {
      return Err(McpError::invalid_request(
        "config updates are disabled in demo mode",
        None,
      ));
    }

    let config = crate::config::proto::Config::from_text(&request.config)
      .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
    let current = self.state.get_config();
    let hash = crate::config::proto::hash_config(&current);
    let (_, secrets) = crate::config::redact_secrets(&current)
      .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    let config =
      crate::config::merge_vault_and_env(config, crate::config::proto::Vault { secrets })
        .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
    self
      .state
      .validate_and_update_config(config, Some(hash))
      .await
      .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
    Ok("Config updated".to_string())
  }
}

#[tool_handler]
impl ServerHandler for TrailBaseMcp {
  fn get_info(&self) -> ServerInfo {
    ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
      .with_server_info(
        Implementation::new("trailbase", env!("CARGO_PKG_VERSION"))
          .with_title("TrailBase MCP")
          .with_description("Native administrative MCP server for TrailBase")
          .with_website_url("https://trailbase.io"),
      )
      .with_instructions(
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

  let protected_mcp =
    Router::new()
      .nest_service("/mcp", service)
      .layer(middleware::from_fn_with_state(
        state.clone(),
        assert_mcp_access,
      ));

  Router::new()
    .merge(protected_mcp)
    .route(
      "/.well-known/oauth-protected-resource",
      get(protected_resource_metadata),
    )
    .route(
      "/.well-known/oauth-protected-resource/mcp",
      get(protected_resource_metadata),
    )
    .route(
      "/.well-known/oauth-authorization-server",
      get(authorization_server_metadata),
    )
    .route("/_/mcp/authorize", get(authorize))
    .route("/_/mcp/callback/{flow}", get(authorization_callback))
    .route("/_/mcp/register", post(register_client))
    .route("/_/mcp/token", post(oauth_token))
}

async fn assert_mcp_access(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Response {
  let authorized = match request.extract_parts_with_state::<User, _>(&state).await {
    Ok(user) => is_admin(&state, &user.uuid).await,
    Err(_) => false,
  };
  if authorized {
    return next.run(request).await;
  }

  let metadata_url = external_url(&state, "/.well-known/oauth-protected-resource");
  let mut response = AuthError::Unauthorized.into_response();
  if let Ok(value) = HeaderValue::from_str(&format!(
    "Bearer resource_metadata=\"{metadata_url}\", scope=\"{MCP_SCOPE}\""
  )) {
    response
      .headers_mut()
      .insert(header::WWW_AUTHENTICATE, value);
  }
  response
}

#[derive(Serialize)]
struct ProtectedResourceMetadata {
  resource: String,
  authorization_servers: Vec<String>,
  scopes_supported: Vec<&'static str>,
  bearer_methods_supported: Vec<&'static str>,
}

async fn protected_resource_metadata(
  State(state): State<AppState>,
) -> AxumJson<ProtectedResourceMetadata> {
  let issuer = external_url(&state, "");
  AxumJson(ProtectedResourceMetadata {
    resource: external_url(&state, MCP_PATH),
    authorization_servers: vec![issuer],
    scopes_supported: vec![MCP_SCOPE],
    bearer_methods_supported: vec!["header"],
  })
}

#[derive(Serialize)]
struct AuthorizationServerMetadata {
  issuer: String,
  authorization_endpoint: String,
  token_endpoint: String,
  registration_endpoint: String,
  response_types_supported: Vec<&'static str>,
  grant_types_supported: Vec<&'static str>,
  code_challenge_methods_supported: Vec<&'static str>,
  token_endpoint_auth_methods_supported: Vec<&'static str>,
  scopes_supported: Vec<&'static str>,
}

async fn authorization_server_metadata(
  State(state): State<AppState>,
) -> AxumJson<AuthorizationServerMetadata> {
  AxumJson(AuthorizationServerMetadata {
    issuer: external_url(&state, ""),
    authorization_endpoint: external_url(&state, "/_/mcp/authorize"),
    token_endpoint: external_url(&state, "/_/mcp/token"),
    registration_endpoint: external_url(&state, "/_/mcp/register"),
    response_types_supported: vec!["code"],
    grant_types_supported: vec!["authorization_code", "refresh_token"],
    code_challenge_methods_supported: vec!["S256"],
    token_endpoint_auth_methods_supported: vec!["none"],
    scopes_supported: vec![MCP_SCOPE],
  })
}

#[derive(Debug, Deserialize, Serialize)]
struct ClientRegistration {
  #[serde(default)]
  redirect_uris: Vec<String>,
  #[serde(default)]
  client_name: Option<String>,
  #[serde(default)]
  scope: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ClientClaims {
  exp: i64,
  iat: i64,
  redirect_uris: Vec<String>,
  client_name: Option<String>,
}

#[derive(Serialize)]
struct ClientRegistrationResponse {
  client_id: String,
  client_id_issued_at: i64,
  redirect_uris: Vec<String>,
  client_name: Option<String>,
  token_endpoint_auth_method: &'static str,
  grant_types: Vec<&'static str>,
  response_types: Vec<&'static str>,
  scope: &'static str,
}

async fn register_client(
  State(state): State<AppState>,
  AxumJson(request): AxumJson<ClientRegistration>,
) -> Result<AxumJson<ClientRegistrationResponse>, OAuthError> {
  if request.redirect_uris.is_empty()
    || request
      .redirect_uris
      .iter()
      .any(|uri| !valid_client_redirect(uri))
    || request
      .scope
      .as_deref()
      .is_some_and(|scope| !scope.split(' ').any(|value| value == MCP_SCOPE))
  {
    return Err(OAuthError::invalid_client_metadata("invalid redirect_uris"));
  }

  let now = chrono::Utc::now().timestamp();
  let claims = ClientClaims {
    iat: now,
    exp: now + chrono::Duration::days(30).num_seconds(),
    redirect_uris: request.redirect_uris.clone(),
    client_name: request.client_name.clone(),
  };
  let client_id = state
    .jwt()
    .encode(&claims)
    .map_err(|err| OAuthError::server(err.to_string()))?;

  Ok(AxumJson(ClientRegistrationResponse {
    client_id,
    client_id_issued_at: now,
    redirect_uris: request.redirect_uris,
    client_name: request.client_name,
    token_endpoint_auth_method: "none",
    grant_types: vec!["authorization_code", "refresh_token"],
    response_types: vec!["code"],
    scope: MCP_SCOPE,
  }))
}

#[derive(Deserialize)]
struct AuthorizeQuery {
  client_id: String,
  redirect_uri: String,
  response_type: String,
  code_challenge: String,
  code_challenge_method: String,
  state: Option<String>,
  scope: Option<String>,
  resource: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
struct FlowClaims {
  exp: i64,
  redirect_uri: String,
  client_id: String,
  state: Option<String>,
}

async fn authorize(
  State(state): State<AppState>,
  user: Option<User>,
  Query(query): Query<AuthorizeQuery>,
) -> Result<Response, OAuthError> {
  let client: ClientClaims = state
    .jwt()
    .decode(&query.client_id)
    .map_err(|_| OAuthError::invalid_request("unknown or expired client_id"))?;
  if query.response_type != "code"
    || query.code_challenge_method != "S256"
    || !client.redirect_uris.contains(&query.redirect_uri)
    || query
      .scope
      .as_deref()
      .is_some_and(|scope| !scope.split(' ').any(|s| s == MCP_SCOPE))
    || query
      .resource
      .as_deref()
      .is_some_and(|resource| resource != external_url(&state, MCP_PATH))
  {
    return Err(OAuthError::invalid_request("invalid authorization request"));
  }

  let flow = state
    .jwt()
    .encode(&FlowClaims {
      exp: chrono::Utc::now().timestamp() + chrono::Duration::minutes(10).num_seconds(),
      redirect_uri: query.redirect_uri,
      client_id: query.client_id,
      state: query.state,
    })
    .map_err(|err| OAuthError::server(err.to_string()))?;
  let callback = format!("/_/mcp/callback/{flow}");
  let login_query = url::form_urlencoded::Serializer::new(String::new())
    .append_pair("redirect_uri", &callback)
    .append_pair("response_type", "code")
    .append_pair("pkce_code_challenge", &query.code_challenge)
    .finish();

  if let Some(user) = user {
    if !is_admin(&state, &user.uuid).await {
      return Err(OAuthError::access_denied(
        "MCP access requires an administrator",
      ));
    }
    let db_user = crate::auth::util::user_by_id(&state, &user.uuid)
      .await
      .map_err(OAuthError::from_auth)?;
    return crate::auth::api::login::build_authorization_code_flow_and_pkce_response(
      &state,
      &db_user,
      callback,
      query.code_challenge,
    )
    .await
    .map_err(OAuthError::from_auth);
  }

  Ok(Redirect::to(&format!("/_/auth/login?{login_query}")).into_response())
}

#[derive(Deserialize)]
struct CallbackQuery {
  code: String,
}

async fn authorization_callback(
  State(state): State<AppState>,
  Path(flow): Path<String>,
  Query(query): Query<CallbackQuery>,
) -> Result<Redirect, OAuthError> {
  let flow: FlowClaims = state
    .jwt()
    .decode(&flow)
    .map_err(|_| OAuthError::invalid_request("unknown or expired authorization flow"))?;
  let mut redirect = url::Url::parse(&flow.redirect_uri)
    .map_err(|_| OAuthError::invalid_request("invalid redirect_uri"))?;
  redirect.query_pairs_mut().append_pair("code", &query.code);
  if let Some(state) = flow.state {
    redirect.query_pairs_mut().append_pair("state", &state);
  }
  Ok(Redirect::to(redirect.as_str()))
}

#[derive(Deserialize)]
struct TokenRequest {
  grant_type: String,
  code: Option<String>,
  code_verifier: Option<String>,
  refresh_token: Option<String>,
  client_id: Option<String>,
  redirect_uri: Option<String>,
  resource: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
  access_token: String,
  token_type: &'static str,
  expires_in: i64,
  refresh_token: Option<String>,
  scope: &'static str,
}

async fn oauth_token(
  State(state): State<AppState>,
  Form(request): Form<TokenRequest>,
) -> Result<AxumJson<TokenResponse>, OAuthError> {
  if request
    .resource
    .as_deref()
    .is_some_and(|resource| resource != external_url(&state, MCP_PATH))
  {
    return Err(OAuthError::invalid_grant("invalid resource"));
  }

  let client_id = request
    .client_id
    .as_deref()
    .ok_or_else(|| OAuthError::invalid_grant("missing client_id"))?;
  let client: ClientClaims = state
    .jwt()
    .decode(client_id)
    .map_err(|_| OAuthError::invalid_grant("unknown or expired client_id"))?;

  let (access_token, refresh_token) = match request.grant_type.as_str() {
    "authorization_code" => {
      let redirect_uri = request
        .redirect_uri
        .as_deref()
        .ok_or_else(|| OAuthError::invalid_grant("missing redirect_uri"))?;
      if !client.redirect_uris.iter().any(|uri| uri == redirect_uri) {
        return Err(OAuthError::invalid_grant(
          "redirect_uri does not match client",
        ));
      }
      let code = request
        .code
        .ok_or_else(|| OAuthError::invalid_grant("missing code"))?;
      let verifier = request
        .code_verifier
        .ok_or_else(|| OAuthError::invalid_grant("missing code_verifier"))?;
      let AxumJson(tokens) = crate::auth::api::token::auth_code_to_token_handler(
        State(state.clone()),
        AxumJson(crate::auth::api::token::AuthCodeToTokenRequest {
          authorization_code: Some(code),
          pkce_code_verifier: Some(verifier),
        }),
      )
      .await
      .map_err(OAuthError::from_auth)?;
      (tokens.auth_token, Some(tokens.refresh_token))
    }
    "refresh_token" => {
      let refresh_token = request
        .refresh_token
        .ok_or_else(|| OAuthError::invalid_grant("missing refresh_token"))?;
      let AxumJson(tokens) = crate::auth::api::refresh::refresh_handler(
        State(state.clone()),
        AxumJson(crate::auth::api::refresh::RefreshRequest { refresh_token }),
      )
      .await
      .map_err(OAuthError::from_auth)?;
      (tokens.auth_token, None)
    }
    _ => return Err(OAuthError::invalid_grant("unsupported grant_type")),
  };
  let claims = crate::auth::AuthTokenClaims::from_auth_token(state.jwt(), &access_token)
    .map_err(|_| OAuthError::server("failed to decode issued access token"))?;

  Ok(AxumJson(TokenResponse {
    access_token,
    token_type: "Bearer",
    expires_in: (claims.exp - chrono::Utc::now().timestamp()).max(0),
    refresh_token,
    scope: MCP_SCOPE,
  }))
}

#[derive(Debug)]
struct OAuthError {
  status: StatusCode,
  code: &'static str,
  description: String,
}

impl OAuthError {
  fn invalid_request(description: impl Into<String>) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "invalid_request",
      description: description.into(),
    }
  }

  fn invalid_client_metadata(description: impl Into<String>) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "invalid_client_metadata",
      description: description.into(),
    }
  }

  fn invalid_grant(description: impl Into<String>) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "invalid_grant",
      description: description.into(),
    }
  }

  fn access_denied(description: impl Into<String>) -> Self {
    Self {
      status: StatusCode::FORBIDDEN,
      code: "access_denied",
      description: description.into(),
    }
  }

  fn server(description: impl Into<String>) -> Self {
    Self {
      status: StatusCode::INTERNAL_SERVER_ERROR,
      code: "server_error",
      description: description.into(),
    }
  }

  fn from_auth(error: AuthError) -> Self {
    Self {
      status: StatusCode::BAD_REQUEST,
      code: "invalid_grant",
      description: error.to_string(),
    }
  }
}

impl IntoResponse for OAuthError {
  fn into_response(self) -> Response {
    (
      self.status,
      AxumJson(json!({ "error": self.code, "error_description": self.description })),
    )
      .into_response()
  }
}

fn valid_client_redirect(uri: &str) -> bool {
  let Ok(uri) = url::Url::parse(uri) else {
    return false;
  };
  uri.scheme() == "https"
    || (uri.scheme() == "http" && matches!(uri.host_str(), Some("localhost" | "127.0.0.1" | "::1")))
}

fn external_url(state: &AppState, path: &str) -> String {
  let mut base = state
    .site_url()
    .as_ref()
    .clone()
    .unwrap_or_else(|| url::Url::parse("http://localhost:4000").expect("constant URL"));
  base.set_path(path);
  base.set_query(None);
  base.set_fragment(None);
  base.to_string().trim_end_matches('/').to_string()
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
  use crate::app_state::test_state;

  #[test]
  fn normalizes_admin_paths() {
    assert_eq!(normalize_admin_path("tables").unwrap(), "/tables");
    assert_eq!(
      normalize_admin_path("/api/_admin/logs/list?limit=5").unwrap(),
      "/logs/list?limit=5"
    );
    assert!(normalize_admin_path("https://example.com").is_err());
  }

  #[tokio::test]
  async fn registers_client_and_builds_pkce_login_redirect() {
    let state = test_state(None).await.unwrap();
    let callback = "http://127.0.0.1:3334/oauth/callback".to_string();
    let AxumJson(registration) = register_client(
      State(state.clone()),
      AxumJson(ClientRegistration {
        redirect_uris: vec![callback.clone()],
        client_name: Some("test client".to_string()),
        scope: Some(MCP_SCOPE.to_string()),
      }),
    )
    .await
    .unwrap();

    let redirect = authorize(
      State(state),
      None,
      Query(AuthorizeQuery {
        client_id: registration.client_id,
        redirect_uri: callback,
        response_type: "code".to_string(),
        code_challenge: "ZmFrZS1jaGFsbGVuZ2U".to_string(),
        code_challenge_method: "S256".to_string(),
        state: Some("client-state".to_string()),
        scope: Some(MCP_SCOPE.to_string()),
        resource: None,
      }),
    )
    .await
    .unwrap()
    .into_response();

    let location = redirect
      .headers()
      .get(header::LOCATION)
      .unwrap()
      .to_str()
      .unwrap();
    assert!(location.starts_with("/_/auth/login?"));
    assert!(location.contains("response_type=code"));
    assert!(location.contains("pkce_code_challenge="));
  }

  #[tokio::test]
  async fn native_tools_use_live_admin_state() {
    let state = test_state(None).await.unwrap();
    let server = TrailBaseMcp::new(state);
    let tool_names: Vec<_> = server
      .tool_router
      .list_all()
      .into_iter()
      .map(|tool| tool.name.to_string())
      .collect();
    assert_eq!(
      tool_names,
      [
        "call_admin_api",
        "execute_sql",
        "get_config",
        "list_tables",
        "update_config"
      ]
    );

    let config = server.get_config().unwrap();
    assert!(config.contains("auth"));
    assert_eq!(
      server
        .update_config(Parameters(ConfigUpdateRequest { config }))
        .await
        .unwrap(),
      "Config updated"
    );

    server
      .execute_sql(Parameters(SqlRequest {
        query: "CREATE TABLE mcp_native_tool_test (id INTEGER PRIMARY KEY)".to_string(),
        attached_databases: None,
      }))
      .await
      .unwrap();
    let tables = server.list_tables().await.unwrap();
    assert!(tables.0.to_string().contains("mcp_native_tool_test"));
  }
}
