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
  if !state
    .access_config(|c| c.auth.enable_anonymous_signin)
    .unwrap_or(false)
  {
    return Err(AuthError::Forbidden);
  }

  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(request.params.redirect_uri))?;

  let create_user = async || -> Result<DbUser, trailbase_sqlite::Error> {
    const INSERT_USER_QUERY: &str =
      formatcp!("INSERT INTO \"{USER_TABLE}\" (username) VALUES (:username) RETURNING * ");

    let username = format!(
      "anon{suffix}",
      suffix = crate::rand::random_numeric_and_lowercase(6)
    );

    return match state
      .user_conn()
      .write_query_value::<DbUser>(
        INSERT_USER_QUERY,
        named_params! {
          ":username": username.clone(),
        },
      )
      .await
    {
      Ok(Some(user)) => Ok(user),
      Ok(None) => Err(trailbase_sqlite::Error::Other("Failed to get user".into())),
      Err(err) => Err(err),
    };
  };

  let mut i = 0;
  loop {
    match create_user().await {
      Ok(user) => {
        return crate::auth::api::login::build_auth_token_flow_response(
          &state,
          &user,
          &cookies,
          redirect_uri,
          json,
        )
        .await;
      }
      Err(_err) => {
        i += 1;
        if i >= 5 {
          return Err(AuthError::Conflict);
        }
      }
    }
  }
}
