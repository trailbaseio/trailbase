#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use askama::Template;
use serde::Deserialize;
use trailbase_wasm::http::{
  Html, HttpError, HttpRoute, IntoBody, IntoResponse, Redirect, Request, Response, StatusCode,
  User, header, routing,
};
use trailbase_wasm::kv::Store;
use trailbase_wasm::{Guest, export};

mod auth;

// Implement the function exported in this world (see above).
struct Endpoints;

impl Guest for Endpoints {
  fn http_handlers() -> Vec<HttpRoute> {
    return vec![
      routing::get(
        LOGIN_UI,
        async |req: Request| -> Result<Response, HttpError> {
          // Read auth config. It may be a bit hacky using KVStore :shrug:.
          let auth_config: auth::AuthConfig = {
            let store = Store::open().map_err(internal)?;
            let value = store
              .get("config:auth")
              .map_err(internal)?
              .ok_or_else(|| internal("missing config"))?;

            if value.is_empty() {
              return Err(internal("empty config"));
            }

            serde_json::from_slice(&value).map_err(internal)?
          };

          return ui_login_handler(auth_config, req.user(), req.query_parse()?).await;
        },
      ),
      routing::get(
        LOGIN_MFA_UI,
        async |req: Request| -> Result<Response, HttpError> {
          return ui_login_mfa_handler(req.user(), req.query_parse()?).await;
        },
      ),
      routing::get(
        "/_/auth/logout",
        async |req: Request| -> Result<Response, HttpError> {
          return Ok(ui_logout_handler(req.query_parse()?).await.into_response());
        },
      ),
      routing::get(
        REGISTER_USER_UI,
        async |req: Request| -> Result<Response, HttpError> {
          return ui_register_handler(req.query_parse()?).await;
        },
      ),
      routing::get(
        "/_/auth/reset_password/request",
        async |req: Request| -> Result<Response, HttpError> {
          return ui_reset_password_request_handler(req.query_parse()?).await;
        },
      ),
      routing::get(
        "/_/auth/reset_password/update/{password_reset_code}",
        async |req: Request| -> Result<Response, HttpError> {
          let password_reset_code = req
            .path_param("password_reset_code")
            .ok_or_else(|| internal("missing code"))?;
          return ui_reset_password_update_handler(req.query_parse()?, password_reset_code).await;
        },
      ),
      routing::get(
        "/_/auth/change_password",
        async |req: Request| -> Result<Response, HttpError> {
          return ui_change_password_handler(req.query_parse()?).await;
        },
      ),
      routing::get(
        "/_/auth/change_email",
        async |req: Request| -> Result<Response, HttpError> {
          let user = req
            .user()
            .ok_or_else(|| HttpError::status(StatusCode::UNAUTHORIZED))?;
          return ui_change_email_handler(req.query_parse()?, user).await;
        },
      ),
      routing::get("/_/auth/{*wildcard}", async |req: Request| {
        return static_assets_handler(
          req
            .path_param("wildcard")
            .ok_or_else(|| internal("missing param"))?,
        )
        .await;
      }),
    ];
  }
}

export!(Endpoints);

#[derive(Debug, Default, Deserialize)]
pub struct LoginQuery {
  redirect_uri: Option<String>,
  mfa_redirect_uri: Option<String>,
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_handler(
  config: auth::AuthConfig,
  user: Option<&User>,
  query: LoginQuery,
) -> Result<Response, HttpError> {
  let redirect_uri = query.redirect_uri.as_deref().unwrap_or(PROFILE_UI);
  if user.is_some() {
    // Already logged in. We rely on this to redirect to profile page (unless another explicit
    // redirect is given) on login success. This way, we can always redirect back to login page
    // both on failure and success rather than conditionally.
    return Ok(Redirect::to(redirect_uri).into_response());
  }

  let oauth_query_params: Vec<(&str, &str)> = [
    Some(("redirect_uri", redirect_uri)),
    query
      .response_type
      .as_ref()
      .map(|r| ("response_type", r.as_str())),
    query
      .pkce_code_challenge
      .as_ref()
      .map(|r| ("pkce_code_challenge", r.as_str())),
  ]
  .into_iter()
  .flatten()
  .collect();

  let html = auth::LoginTemplate {
    state: [
      auth::hidden_input("redirect_uri", Some(redirect_uri)),
      auth::hidden_input(
        "mfa_redirect_uri",
        Some(query.mfa_redirect_uri.as_deref().unwrap_or(LOGIN_MFA_UI)),
      ),
      auth::hidden_input("response_type", query.response_type.as_ref()),
      auth::hidden_input("pkce_code_challenge", query.pkce_code_challenge.as_ref()),
    ]
    .join("\n"),
    alert: query.alert.as_deref().unwrap_or_default(),
    enable_registration: !config.disable_password_auth,
    oauth_providers: &config.oauth_providers,
    oauth_query_params: &oauth_query_params,
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct LoginMfaQuery {
  mfa_token: String,
  redirect_uri: Option<String>,
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_mfa_handler(
  user: Option<&User>,
  query: LoginMfaQuery,
) -> Result<Response, HttpError> {
  let redirect_uri = query.redirect_uri.as_deref().unwrap_or(PROFILE_UI);
  if user.is_some() {
    // Already logged in. We rely on this to redirect to profile page (unless another explicit
    // redirect is given) on login success. This way, we can always redirect back to login page
    // both on failure and success rather than conditionally.
    return Ok(Redirect::to(redirect_uri).into_response());
  }

  let html = auth::LoginMfaTemplate {
    state: [
      auth::hidden_input("mfa_token", Some(query.mfa_token)),
      auth::hidden_input("redirect_uri", Some(redirect_uri)),
      auth::hidden_input("response_type", query.response_type.as_ref()),
      auth::hidden_input("pkce_code_challenge", query.pkce_code_challenge.as_ref()),
    ]
    .join("\n"),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutQuery {
  redirect_uri: Option<String>,
}

async fn ui_logout_handler(query: LogoutQuery) -> Redirect {
  let redirect_uri = query.redirect_uri.as_deref().unwrap_or(LOGIN_UI);
  return Redirect::to(&format!("/api/auth/v1/logout?redirect_uri={redirect_uri}"));
}

#[derive(Debug, Default, Deserialize)]
pub struct RegisterQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_register_handler(query: RegisterQuery) -> Result<Response, HttpError> {
  let redirect_uri = query.redirect_uri.as_deref().unwrap_or(LOGIN_UI);
  let html = auth::RegisterTemplate {
    state: auth::redirect_uri(Some(redirect_uri)),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordRequestQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_request_handler(
  query: ResetPasswordRequestQuery,
) -> Result<Response, HttpError> {
  let redirect_uri = query.redirect_uri.as_deref().unwrap_or(LOGIN_UI);
  let html = auth::ResetPasswordRequestTemplate {
    state: auth::redirect_uri(Some(redirect_uri)),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordUpdateQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_update_handler(
  query: ResetPasswordUpdateQuery,
  password_reset_code: &str,
) -> Result<Response, HttpError> {
  let redirect_uri = auth::hidden_input("redirect_uri", query.redirect_uri.as_ref());
  let password_reset_code = auth::hidden_input("password_reset_code", Some(&password_reset_code));

  let html = auth::ResetPasswordUpdateTemplate {
    state: format!("{redirect_uri}\n{password_reset_code}"),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangePasswordQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_change_password_handler(query: ChangePasswordQuery) -> Result<Response, HttpError> {
  let html = auth::ChangePasswordTemplate {
    state: auth::redirect_uri(query.redirect_uri.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangeEmailQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_change_email_handler(
  query: ChangeEmailQuery,
  user: &User,
) -> Result<Response, HttpError> {
  let redirect_uri = auth::hidden_input("redirect_uri", query.redirect_uri.as_ref());
  let csrf_token = auth::hidden_input("csrf_token", Some(&user.csrf_token));

  let html = auth::ChangeEmailTemplate {
    state: format!("{redirect_uri}\n{csrf_token}"),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

async fn static_assets_handler(path: &str) -> Result<Response, HttpError> {
  // We want as little magic as possible. The only /_/auth/subpath that isn't SSR, is
  // profile, so we when hitting /profile or /profile, we want actually want to serve
  // the static profile/index.html.
  let file = match path {
    "profile" => auth::AuthAssets::get("profile/index.html"),
    p => auth::AuthAssets::get(p),
  }
  .ok_or_else(|| HttpError::message(StatusCode::NOT_FOUND, "Not found"))?;

  let response_builder = Response::builder()
    .header(header::CACHE_CONTROL, "public")
    .header(header::CACHE_CONTROL, "max-age=604800")
    .header(header::CACHE_CONTROL, "immutable")
    .header(header::CONTENT_TYPE, file.metadata.mimetype());

  return response_builder
    .body(file.data.into_body())
    .map_err(internal);
}

#[allow(unused)]
fn internal(err: impl std::string::ToString) -> HttpError {
  return HttpError::message(StatusCode::INTERNAL_SERVER_ERROR, err);
}

const LOGIN_UI: &str = "/_/auth/login";
const LOGIN_MFA_UI: &str = "/_/auth/login_mfa";
const PROFILE_UI: &str = "/_/auth/profile";
const REGISTER_USER_UI: &str = "/_/auth/register";
