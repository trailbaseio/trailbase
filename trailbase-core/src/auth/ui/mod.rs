use axum::extract::Query;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use lazy_static::lazy_static;
use minijinja::{context, Environment};
use reqwest::StatusCode;
use rust_embed::RustEmbed;
use serde::Deserialize;

use crate::assets::{cow_to_string, AssetService};
use crate::auth::User;

fn build_env() -> Result<Environment<'static>, minijinja::Error> {
  fn get(fname: &str) -> String {
    let file = AuthAssets::get(fname).expect("startup");
    cow_to_string(file.data)
  }

  lazy_static! {
    static ref login_template: String = get("login/index.html");
    static ref register_template: String = get("register/index.html");
    static ref reset_password_request_template: String = get("reset_password/request/index.html");
    static ref reset_password_update_template: String = get("reset_password/update/index.html");
    static ref change_password_template: String = get("change_password/index.html");
    static ref change_email_template: String = get("change_email/index.html");
  }

  let mut env = Environment::new();

  env.add_template("login", &login_template)?;
  env.add_template("register", &register_template)?;
  env.add_template("reset_password_request", &reset_password_request_template)?;
  env.add_template("reset_password_update", &reset_password_update_template)?;
  env.add_template("change_password", &change_password_template)?;
  env.add_template("change_email", &change_email_template)?;

  return Ok(env);
}

fn templates() -> &'static Environment<'static> {
  lazy_static! {
    static ref env: Environment<'static> = build_env().expect("startup");
  }

  return &env;
}

#[derive(Debug, Default, Deserialize)]
pub struct LoginQuery {
  redirect_to: Option<String>,
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_handler(Query(query): Query<LoginQuery>, user: Option<User>) -> Response {
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

  let ctx = context! {
    alert => query.alert.as_deref().unwrap_or_default(),
    state => form_state,
  };

  return match templates()
    .get_template("login")
    .expect("infallible")
    .render(ctx)
  {
    Ok(output) => Html(output).into_response(),
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
  return match templates().get_template("register").expect("infallible").render(context! {
    alert => query.alert.as_deref().unwrap_or_default(),
    state => query.redirect_to.map(|r| format!("<input name=\"redirect_to\" type=\"hidden\" value=\"{r}\" />")),
  }) {
    Ok(output) => Html(output).into_response(),
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
  return match templates()
    .get_template("reset_password_request")
    .expect("infallible")
    .render(context! {
      alert => query.alert.as_deref().unwrap_or_default(),
      state => query.redirect_to.map(|r| format!("<input name=\"redirect_to\" type=\"hidden\" value=\"{r}\" />")),
    }) {
    Ok(output) => Html(output).into_response(),
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
  return match templates()
    .get_template("reset_password_update")
    .expect("infallible")
    .render(context! {
      alert => query.alert.as_deref().unwrap_or_default(),
      state => query.redirect_to.map(|r| format!("<input name=\"redirect_to\" type=\"hidden\" value=\"{r}\" />")),
    }) {
    Ok(output) => Html(output).into_response(),
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
  return match templates().get_template("change_password").expect("infallible").render(context! {
    alert => query.alert.as_deref().unwrap_or_default(),
    state => query.redirect_to.map(|r| format!("<input name=\"redirect_to\" type=\"hidden\" value=\"{r}\" />")),
  }) {
    Ok(output) => Html(output).into_response(),
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

  return match templates()
    .get_template("change_email")
    .expect("infallible")
    .render(context! {
      alert => query.alert.as_deref().unwrap_or_default(),
      state => form_state,
    }) {
    Ok(output) => Html(output).into_response(),
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
pub(crate) fn auth_ui_router() -> Router<crate::AppState> {
  // Static assets for auth UI .
  let serve_auth_assets = AssetService::<AuthAssets>::with_parameters(
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

fn hidden_input(name: &str, value: Option<&String>) -> String {
  if let Some(value) = value {
    return format!("<input name=\"{name}\" type=\"hidden\" value=\"{value}\" />");
  }
  return "".to_string();
}

#[derive(RustEmbed, Clone)]
#[folder = "js/auth/dist/"]
struct AuthAssets;
