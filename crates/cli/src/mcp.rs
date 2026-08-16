use rmcp::model::*;
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use rmcp_openapi::Server as OpenApiServer;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use trailbase_client::{Client, Tokens};

/// Wraps `rmcp_openapi::Server` for us to inject per-request tokens and allow to refresh stale
/// tokens.
pub struct McpServer {
  inner: Mutex<OpenApiServer>,
  client: Client,
}

impl McpServer {
  pub fn from(client: Client, server: OpenApiServer) -> Self {
    return Self {
      inner: Mutex::new(server),
      client,
    };
  }

  async fn update_tokens(&self, inner: &mut OpenApiServer) -> Result<(), McpError> {
    use reqwest::header::{AUTHORIZATION, HeaderValue};

    let Some(Tokens { auth_token, .. }) = self.client.tokens() else {
      return Err(McpError::internal_error("Not authenticated?", None));
    };

    let jwt: JwtTokenClaims = decode_auth_token(&auth_token)?;
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .expect("Duration since epoch")
      .as_secs() as i64;

    if jwt.exp - 20 > now {
      // Still valid for another 20 seconds, no refresh needed
      return Ok(());
    }

    let refreshed = self
      .client
      .refresh()
      .await
      .map_err(|err| McpError::internal_error(err.to_string(), None))?;
    debug_assert!(refreshed, "expected refresh");

    let Some(Tokens {
      auth_token,
      refresh_token,
      csrf_token,
    }) = self.client.tokens()
    else {
      return Err(McpError::internal_error("Not authenticated?", None));
    };

    fn header_value(value: &str) -> Result<HeaderValue, McpError> {
      return HeaderValue::from_str(value)
        .map_err(|err| McpError::internal_error(err.to_string(), None));
    }

    let headers = inner.default_headers.get_or_insert_default();
    headers.insert(
      AUTHORIZATION,
      header_value(&format!("Bearer {auth_token}"))?,
    );
    if let Some(refresh_token) = refresh_token {
      headers.insert("Refresh-Token", header_value(&refresh_token)?);
    }
    if let Some(csrf_token) = csrf_token {
      headers.insert("CSRF-Token", header_value(&csrf_token)?);
    }

    return Ok(());
  }

  // async fn add_current_auth_token_to_context(
  //   &self,
  //   mut context: RequestContext<RoleServer>,
  // ) -> Result<RequestContext<RoleServer>, McpError> {
  //   use rmcp_actix_web::transport::AuthorizationHeader;
  //
  //   // TODO: We can be smarter here and only refresh when token is close to expiry.
  //   let refreshed = self
  //     .client
  //     .refresh()
  //     .await
  //     .map_err(|err| McpError::internal_error(err.to_string(), None))?;
  //
  //   debug_assert!(refreshed, "expected refresh");
  //
  //   let Some(Tokens { auth_token, .. }) = self.client.tokens() else {
  //     return Err(McpError::internal_error("Missing tokens", None));
  //   };
  //
  //   context
  //     .extensions
  //     .insert(AuthorizationHeader(format!("Bearer {auth_token}")));
  //
  //   return Ok(context);
  // }
}

impl ServerHandler for McpServer {
  fn get_info(&self) -> ServerInfo {
    self.inner.blocking_lock().get_info()
  }

  async fn initialize(
    &self,
    request: InitializeRequestParams,
    context: RequestContext<RoleServer>,
  ) -> Result<InitializeResult, McpError> {
    let mut lock = self.inner.lock().await;
    self.update_tokens(&mut lock).await?;
    return lock.initialize(request, context).await;
  }

  async fn list_tools(
    &self,
    request: Option<PaginatedRequestParams>,
    context: RequestContext<RoleServer>,
  ) -> Result<ListToolsResult, McpError> {
    let mut lock = self.inner.lock().await;
    self.update_tokens(&mut lock).await?;
    return lock.list_tools(request, context).await;
  }

  async fn call_tool(
    &self,
    request: CallToolRequestParams,
    context: RequestContext<RoleServer>,
  ) -> Result<CallToolResult, McpError> {
    let mut lock = self.inner.lock().await;
    self.update_tokens(&mut lock).await?;
    return lock.call_tool(request, context).await;
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct JwtTokenClaims {
  sub: String,
  iat: i64,
  exp: i64,
}

fn decode_auth_token<T: serde::de::DeserializeOwned + Clone>(token: &str) -> Result<T, McpError> {
  return jsonwebtoken::dangerous::insecure_decode::<T>(token)
    .map(|data| data.claims)
    .map_err(|err| McpError::internal_error(err.to_string(), None));
}
