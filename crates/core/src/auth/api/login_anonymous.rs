use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::api::register::RegisterUserParams;
use crate::auth::user::DbUser;
use crate::auth::util::validate_redirect;
use crate::config::proto::UserIdentifier;
use crate::constants::USER_TABLE;
use crate::extract::Either;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
pub struct LoginAnonymousRequest {
  #[serde(flatten)]
  pub params: RegisterUserParams,
}

/// Registers a new user with email and password.
#[utoipa::path(
  post,
  path = "/login_anonymous",
  tag = "auth",
  params(RegisterUserParams),
  request_body = LoginAnonymousRequest,
  responses(
    (status = 303, description = "Form fail OR success, new user registered, or user already exists."),
    (status = 424, description = "Failed to send verification Email."),
  )
)]
pub async fn login_anonymous_user_handler(
  State(state): State<AppState>,
  Query(query): Query<RegisterUserParams>,
  cookies: Cookies,
  either_request: Either<LoginAnonymousRequest>,
) -> Result<Response, AuthError> {
  let (disabled, enable_anonymous, user_identifier) = state.access_config(|c| {
    let auth = &c.auth;
    (
      auth.disable_password_auth.unwrap_or(false),
      auth.enable_anonymous_signin.unwrap_or(false),
      auth.user_identifier,
    )
  });
  if disabled && !enable_anonymous {
    return Err(AuthError::Forbidden);
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.params.redirect_uri))?;

  match user_identifier
    .and_then(|ui| ui.try_into().ok())
    .unwrap_or(UserIdentifier::Undefined)
  {
    UserIdentifier::Undefined
    | UserIdentifier::OnlyEmail
    | UserIdentifier::RequireEmail
    | UserIdentifier::RequireEmailAndUsername => {
      if !json && let Some(redirect_uri) = redirect_uri {
        return Ok(
          Redirect::to(&format!("{redirect_uri}?alert={msg}", msg = urlencode(""))).into_response(),
        );
      }
      return Err(AuthError::FailedDependency("not supported".into()));
    }
    _ => {}
  };

  // let success_response = {
  //   let redirect_uri = redirect_uri.clone();
  //   move || {
  //     return match redirect_uri {
  //       Some(ref redirect) => Redirect::to(redirect).into_response(),
  //       _ => (StatusCode::OK, "registered").into_response(),
  //     };
  //   }
  // };

  let username = format!(
    "anon{suffix}",
    suffix = crate::rand::random_numeric_and_lowercase(6)
  );

  const INSERT_USER_QUERY: &str =
    formatcp!("INSERT INTO \"{USER_TABLE}\" (username) VALUES (:username) RETURNING * ");

  let user = match state
    .user_conn()
    .write_query_value::<DbUser>(
      INSERT_USER_QUERY,
      named_params! {
        ":username": username,
      },
    )
    .await
  {
    Ok(Some(user)) => user,
    Err(err) => {
      return Err(AuthError::Internal(err.into()));
    }
    Ok(None) => {
      return Err(AuthError::Internal("Failed to get user".into()));
    }
  };

  return crate::auth::api::login::build_auth_token_flow_response(
    &state,
    &user,
    &cookies,
    redirect_uri,
    json,
  )
  .await;
}
