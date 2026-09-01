use axum::{Json, extract::State};
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use trailbase_sqlite::named_params;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::jwt::EmailVerificationTokenClaims;
use crate::auth::password::{hash_password, validate_password_policy};
use crate::auth::user::DbUser;
use crate::auth::util::{
  user_by_email, user_by_username, validate_and_normalize_email_address,
  validate_and_normalize_username,
};
use crate::constants::USER_TABLE;
use crate::email::Email;

#[derive(Debug, Serialize, Deserialize, Default, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct CreateUserRequest {
  #[ts(optional)]
  pub email: Option<String>,
  #[ts(optional)]
  pub username: Option<String>,
  pub password: String,
  /// Whether above email should be considers verified.
  pub verified: bool,
  pub admin: bool,
}

#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct CreateUserResponse {
  pub id: Uuid,
}

#[utoipa::path(
  post,
  path = "/user",
  tag = "admin",
  request_body = CreateUserRequest,
  responses(
    (status = 200, description = "Success", body = CreateUserResponse),
  )
)]
pub async fn create_user_handler(
  State(state): State<AppState>,
  Json(request): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, Error> {
  let normalized_email = request
    .email
    .map(|email| validate_and_normalize_email_address(&email))
    .transpose()?;
  let normalized_username = request
    .username
    .map(|name| validate_and_normalize_username(&name))
    .transpose()?;

  let auth_options = state.auth_options();
  validate_password_policy(
    &request.password,
    &request.password,
    auth_options.password_options(),
  )?;

  match (&normalized_email, &normalized_username) {
    (Some(email), _) if user_by_email(&state, email).await.is_ok() => {
      return Err(Error::AlreadyExists("user"));
    }
    (_, Some(name)) if user_by_username(&state, name).await.is_ok() => {
      return Err(Error::AlreadyExists("user"));
    }
    (None, None) => {
      return Err(Error::Other("Need email and/or username".into()));
    }
    _ => {}
  };

  let hashed_password = hash_password(&request.password)?;

  const INSERT_USER_QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{USER_TABLE}\" \
        (email, unverified_email, username, password_hash, admin) \
      VALUES \
        (:email, :unverified_email, :username, :password_hash, :admin) \
      RETURNING * \
    ",
  );

  let Some(user) = state
    .user_conn()
    .write_query_value::<DbUser>(
      INSERT_USER_QUERY,
      named_params! {
        ":email": if request.verified {
            normalized_email.clone()
          } else {
            None
          },
        ":unverified_email": if request.verified {
            None
          } else {
            normalized_email
          },
        ":username": normalized_username,
        ":password_hash": hashed_password,
        ":admin": request.admin,
      },
    )
    .await?
  else {
    return Err(Error::Precondition("Internal".into()));
  };

  // Send an email
  if let Some(ref email) = user.email
    && !request.verified
  {
    let claims =
      EmailVerificationTokenClaims::new(&user.uuid(), email.clone(), chrono::Duration::hours(4));

    let token = state
      .jwt()
      .encode(&claims)
      .map_err(|err| Error::Internal(err.into()))?;

    // NOTE: We cannot pass a valid redirect_uri, since we cannot be sure if auth UI is
    // installed.
    Email::verification_email(&state, email, &token, None)?
      .send()
      .await?;
  }

  return Ok(Json(CreateUserResponse {
    id: Uuid::from_bytes(user.id),
  }));
}

#[cfg(test)]
pub(crate) async fn create_user_for_test(
  state: &AppState,
  email: &str,
  password: &str,
) -> Result<Uuid, Error> {
  let response = create_user_handler(
    State(state.clone()),
    Json(CreateUserRequest {
      email: Some(email.to_string()),
      username: None,
      password: password.to_string(),
      verified: true,
      admin: false,
    }),
  )
  .await
  .unwrap();

  return Ok(response.id);
}
