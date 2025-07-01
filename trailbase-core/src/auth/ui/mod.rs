use askama::Template;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use reqwest::StatusCode;
use serde::Deserialize;
use trailbase_assets::AssetService;
use trailbase_assets::auth::{
  ChangeEmailTemplate, ChangePasswordTemplate, LoginTemplate, RegisterTemplate,
  ResetPasswordRequestTemplate, ResetPasswordUpdateTemplate, hidden_input, redirect_to,
};

use crate::AppState;
use crate::auth::User;

#[derive(Debug, Default, Deserialize)]
pub struct LoginQuery {
  redirect_to: Option<String>,
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_handler(
  State(state): State<AppState>,
  Query(query): Query<LoginQuery>,
  user: Option<User>,
) -> Response {
  if user.is_some() {
    // Already logged in.
    return Redirect::to("/_/auth/profile").into_response();
  }

  let form_state = indoc::formatdoc!(
    r#"
    {redirect_to}
    {response_type}
    {pkce_code_challenge}
    "#,
    redirect_to = hidden_input("redirect_to", query.redirect_to.as_ref()),
    response_type = hidden_input("response_type", query.response_type.as_ref()),
    pkce_code_challenge = hidden_input("pkce_code_challenge", query.pkce_code_challenge.as_ref()),
  );

  let html = LoginTemplate {
    state: form_state,
    alert: query.alert.as_deref().unwrap_or_default(),
    redirect_to: query.redirect_to.as_deref(),
    enable_registration: !state.access_config(|c| c.auth.disable_password_auth.unwrap_or(false)),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct RegisterQuery {
  redirect_to: Option<String>,
  alert: Option<String>,
}

async fn ui_register_handler(Query(query): Query<RegisterQuery>) -> Response {
  let html = RegisterTemplate {
    state: redirect_to(query.redirect_to.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordRequestQuery {
  redirect_to: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_request_handler(
  Query(query): Query<ResetPasswordRequestQuery>,
) -> Response {
  let html = ResetPasswordRequestTemplate {
    state: redirect_to(query.redirect_to.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordUpdateQuery {
  redirect_to: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_update_handler(
  Query(query): Query<ResetPasswordUpdateQuery>,
) -> Response {
  let html = ResetPasswordUpdateTemplate {
    state: redirect_to(query.redirect_to.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangePasswordQuery {
  redirect_to: Option<String>,
  alert: Option<String>,
}

async fn ui_change_password_handler(Query(query): Query<ChangePasswordQuery>) -> Response {
  let html = ChangePasswordTemplate {
    state: redirect_to(query.redirect_to.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangeEmailQuery {
  redirect_to: Option<String>,
  alert: Option<String>,
}

async fn ui_change_email_handler(Query(query): Query<ChangeEmailQuery>, user: User) -> Response {
  let form_state = indoc::formatdoc!(
    r#"
    {redirect_to}
    {csrf_token}
  "#,
    redirect_to = hidden_input("redirect_to", query.redirect_to.as_ref()),
    csrf_token = hidden_input("csrf_token", Some(&user.csrf_token)),
  );

  let html = ChangeEmailTemplate {
    state: form_state,
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutQuery {
  redirect_to: Option<String>,
}

async fn ui_logout_handler(Query(query): Query<LogoutQuery>) -> Redirect {
  if let Some(redirect_to) = query.redirect_to {
    return Redirect::to(&format!("/api/auth/v1/logout?redirect_to={redirect_to}"));
  }
  return Redirect::to("/api/auth/v1/logout");
}

/// HTML endpoints of core auth functionality.
pub(crate) fn auth_ui_router() -> Router<AppState> {
  // Static assets for auth UI .
  let serve_auth_assets = AssetService::<trailbase_assets::AuthAssets>::with_parameters(
    // We want as little magic as possible. The only /_/auth/subpath that isn't SSR, is profile, so
    // we when hitting /profile or /profile, we want actually want to serve the static
    // profile/index.html.
    Some(Box::new(|path| {
      if path == "profile" {
        Some(format!("{path}/index.html"))
      } else {
        None
      }
    })),
    None,
  );

  return Router::new()
    .route("/_/auth/login", get(ui_login_handler))
    .route("/_/auth/logout", get(ui_logout_handler))
    .route("/_/auth/register", get(ui_register_handler))
    .route(
      "/_/auth/reset_password/request",
      get(ui_reset_password_request_handler),
    )
    .route(
      "/_/auth/reset_password/update",
      get(ui_reset_password_update_handler),
    )
    .route("/_/auth/change_password", get(ui_change_password_handler))
    .route("/_/auth/change_email", get(ui_change_email_handler))
    .nest_service("/_/auth/", serve_auth_assets);
}
