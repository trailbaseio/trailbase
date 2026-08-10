use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use const_format::formatcp;
use serde::Deserialize;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::util::{validate_and_normalize_username, validate_redirect};
use crate::auth::{AuthError, User};
use crate::config::proto::UserIdentifier;
use crate::constants::USER_TABLE;
use crate::extract::Either;
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams, ToSchema, TS)]
pub(crate) struct ChangeUsernameParams {
  /// Success (and error if err_redirect_uri not present) redirect target for non-JSON requests.
  pub redirect_uri: Option<String>,
  /// Error redirect target for non-JSON requests.
  pub err_redirect_uri: Option<String>,
}

#[derive(Debug, Default, Deserialize, ToSchema, TS)]
#[ts(export)]
pub struct ChangeUsernameRequest {
  pub new_username: Option<String>,

  #[serde(flatten)]
  pub params: ChangeUsernameParams,
}

/// Request a change of a user username.
#[utoipa::path(
  post,
  path = "/change_username",
  tag = "auth",
  params(ChangeUsernameParams),
  request_body = ChangeUsernameRequest,
  responses(
    (status = 200, description = "Success, when redirect_uri not present."),
    (status = 303, description = "Success, when redirect_uri present."),
    (status = 409, description = "Fail, username already taken."),
    (status = 500, description = "Fail, something went wrong."),
  )
)]
pub async fn change_username_handler(
  State(state): State<AppState>,
  Query(query): Query<ChangeUsernameParams>,
  user: User,
  either_request: Either<ChangeUsernameRequest>,
) -> Result<Response, AuthError> {
  if state.demo_mode() {
    return Err(AuthError::BadRequest("Disallowed in demo"));
  }

  let (
    ChangeUsernameRequest {
      new_username,
      params,
    },
    json,
  ) = match either_request {
    Either::Json(req) => (req, true),
    Either::Multipart(req, _) => (req, false),
    Either::Form(req) => (req, false),
  };

  let redirect_uri = validate_redirect(&state, query.redirect_uri.or(params.redirect_uri))?;
  let err_redirect_uri =
    validate_redirect(&state, query.err_redirect_uri.or(params.err_redirect_uri))?;

  let user_identifier = state
    .access_config(|c| c.auth.user_identifier)
    .and_then(|ui| ui.try_into().ok())
    .unwrap_or(UserIdentifier::Undefined);

  let new_username = match (new_username, user_identifier, user.email.as_ref()) {
    (
      Some(new_username),
      UserIdentifier::RequireEmail
      | UserIdentifier::RequireUsername
      | UserIdentifier::OnlyUsername
      | UserIdentifier::RequireEmailAndUsername,
      _email,
    ) => match validate_and_normalize_username(&new_username) {
      Ok(username) => Some(username),
      Err(_err)
        if new_username.is_empty() && matches!(user_identifier, UserIdentifier::RequireEmail) =>
      {
        None
      }
      Err(err) => {
        if !json && let Some(ref redirect_uri) = err_redirect_uri.or(redirect_uri) {
          const MSG: &str = "Invalid username";
          return Ok(
            Redirect::to(&format!("{redirect_uri}?alert={msg}", msg = urlencode(MSG)))
              .into_response(),
          );
        }

        return Err(err);
      }
    },
    (Some(_), _, _) => {
      return Err(AuthError::BadRequest("Cannot change username"));
    }
    (
      None,
      UserIdentifier::Undefined | UserIdentifier::OnlyEmail | UserIdentifier::RequireEmail,
      Some(email),
    ) if !email.is_empty() => None,
    (None, _, _) => {
      return Err(AuthError::BadRequest("Cannot unset username"));
    }
  };

  const UPDATE_USERNAME_QUERY: &str = formatcp!(
    "\
      UPDATE \"{USER_TABLE}\" \
        SET username = :new_username \
      WHERE \
        id = :id; \
    "
  );

  let Ok(rows_affected) = state
    .user_conn()
    .execute(
      UPDATE_USERNAME_QUERY,
      named_params! {
        ":new_username": new_username,
        ":id": user.uuid.as_bytes().to_vec(),
      },
    )
    .await
  else {
    if !json && let Some(ref redirect_uri) = err_redirect_uri.or(redirect_uri) {
      const MSG: &str = "username already taken";
      return Ok(
        Redirect::to(&format!("{redirect_uri}?alert={msg}", msg = urlencode(MSG))).into_response(),
      );
    }
    // Insert will fail if the username isn't unique.
    return Err(AuthError::Conflict);
  };

  return match rows_affected {
    0 => {
      // No need to redirect, this is a freak error.
      Err(AuthError::Internal(
        "update failed, invalid user id?".into(),
      ))
    }
    1 => {
      if !json && let Some(redirect_uri) = redirect_uri {
        Ok(Redirect::to(&redirect_uri).into_response())
      } else {
        Ok(StatusCode::OK.into_response())
      }
    }
    _ => {
      panic!("username update affected multiple rows: {rows_affected}");
    }
  };
}
