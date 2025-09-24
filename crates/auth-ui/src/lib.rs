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
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_handler(
  config: auth::AuthConfig,
  user: Option<&User>,
  query: LoginQuery,
) -> Result<Response, HttpError> {
  if query.redirect_uri.is_none() && user.is_some() {
    // Already logged in. Only redirect to profile-page if no explicit other redirect is provided.
    // For example, if we're already logged in the browser but want to sign-in with the browser
    // from an app, we still have to go through the motions of signing in.
    //
    // QUESTION: Too much magic, just remove?
    return Ok(Redirect::to(PROFILE_UI).into_response());
  }

  let redirect_uri = auth::hidden_input("redirect_uri", query.redirect_uri.as_ref());
  let response_type = auth::hidden_input("response_type", query.response_type.as_ref());
  let pkce_code_challenge =
    auth::hidden_input("pkce_code_challenge", query.pkce_code_challenge.as_ref());

  let oauth_query_params: Vec<(&str, &str)> = [
    query
      .redirect_uri
      .as_ref()
      .map(|r| ("redirect_uri", r.as_str())),
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
    state: format!("{redirect_uri}\n{response_type}\n{pkce_code_challenge}"),
    alert: query.alert.as_deref().unwrap_or_default(),
    enable_registration: !config.disable_password_auth,
    oauth_providers: &config.oauth_providers,
    oauth_query_params: &oauth_query_params,
  }
  .render();

  return Ok(Html(html.map_err(internal)?).into_response());
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutQuery {
  redirect_uri: Option<String>,
}

async fn ui_logout_handler(query: LogoutQuery) -> Redirect {
  if let Some(redirect_uri) = query.redirect_uri {
    return Redirect::to(&format!("/api/auth/v1/logout?redirect_uri={redirect_uri}"));
  }
  return Redirect::to("/api/auth/v1/logout");
}

#[derive(Debug, Default, Deserialize)]
pub struct RegisterQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_register_handler(query: RegisterQuery) -> Result<Response, HttpError> {
  let html = auth::RegisterTemplate {
    state: auth::redirect_uri(query.redirect_uri.as_ref()),
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
  let html = auth::ResetPasswordRequestTemplate {
    state: auth::redirect_uri(query.redirect_uri.as_ref()),
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
const PROFILE_UI: &str = "/_/auth/profile";
const REGISTER_USER_UI: &str = "/_/auth/register";
