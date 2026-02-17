use axum::{
  extract::{Json, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};
use chrono::Duration;
use const_format::formatcp;
use hmac::{Hmac, Mac};
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use trailbase_sqlite::params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::api::login::LoginResponse;
use crate::auth::tokens::mint_new_tokens;
use crate::auth::util::{user_by_email, validate_and_normalize_email_address};
use crate::auth::{AuthError, User};
use crate::auth::user::DbUser;
use crate::auth::password::check_user_password;
use crate::constants::{OTP_LENGTH, USER_TABLE};
use crate::email::Email;
use crate::rand::generate_random_string;

const OTP_TTL_SEC: i64 = 300; // 5 minutes
const OTP_RATE_LIMIT_SEC: i64 = 60; // 1 minute

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct RequestOTPRequest {
  pub email: String,
}

#[utoipa::path(
  post,
  path = "/otp/request",
  tag = "auth",
  request_body = RequestOTPRequest,
  responses(
    (status = 200, description = "OTP sent.")
  )
)]

pub async fn request_otp_handler(
  State(state): State<AppState>,
  Json(params): Json<RequestOTPRequest>,
) -> Result<Response, AuthError> {
  let email = validate_and_normalize_email_address(&params.email)?;
  let db_user = user_by_email(&state, &email).await?;

  if let Some(last_sent) = db_user.otp_sent_at {
    let Some(timestamp) = chrono::DateTime::from_timestamp(last_sent, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };
    let age: Duration = chrono::Utc::now() - timestamp;
    if age < Duration::seconds(OTP_RATE_LIMIT_SEC) {
      return Err(AuthError::TooManyRequests);
    }
  }

  let otp_code = generate_random_string(OTP_LENGTH);
  const UPDATE_OTP_QUERY: &str = formatcp!(
    "\
      UPDATE '{USER_TABLE}' \
      SET \
        otp_code = $1, \
        otp_sent_at = UNIXEPOCH() \
      WHERE \
        id = $2 \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(UPDATE_OTP_QUERY, params!(otp_code.clone(), db_user.id))
    .await?;

  if rows_affected != 1 {
    return Err(AuthError::Internal("Failed to update user OTP".into()));
  }

  let email = Email::otp_email(&state, &db_user.email, &otp_code)
    .map_err(|err| AuthError::Internal(err.into()))?;
  email
    .send()
    .await
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok((StatusCode::OK, "OTP sent").into_response());
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct VerifyOTPRequest {
  pub email: String,
  pub code: String,
}

#[utoipa::path(
  post,
  path = "/otp/verify",
  tag = "auth",
  request_body = VerifyOTPRequest,
  responses(
    (status = 200, description = "Auth tokens.", body = LoginResponse)
  )
)]
pub async fn verify_otp_handler(
  State(state): State<AppState>,
  Json(params): Json<VerifyOTPRequest>,
) -> Result<Response, AuthError> {
  let email = validate_and_normalize_email_address(&params.email)?;
  let db_user = user_by_email(&state, &email).await?;

  verify_otp_code(&db_user, &params.code)?;

  if db_user.totp_secret.is_some() {
    return Err(AuthError::BadRequest("TOTP_REQUIRED"));
  }

  let mut updated_user = db_user.clone();
  updated_user.verified = true;

  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  
  let tokens = mint_new_tokens(state.user_conn(), &updated_user, auth_token_ttl).await?;
  
  let response = LoginResponse {
    auth_token: state
      .jwt()
      .encode(&tokens.auth_token_claims)
      .map_err(|err| AuthError::Internal(err.into()))?,
    refresh_token: tokens.refresh_token,
    csrf_token: tokens.auth_token_claims.csrf_token,
  };
  
  return Ok(Json(response).into_response());
}

pub fn verify_otp_code(user: &DbUser, code: &str) -> Result<(), AuthError> {
  if let (Some(expected_code), Some(sent_at)) = (&user.otp_code, user.otp_sent_at) {
    if expected_code != code {
      return Err(AuthError::BadRequest("invalid user"));
    }

    let Some(timestamp) = chrono::DateTime::from_timestamp(sent_at, 0) else {
      return Err(AuthError::Internal("Invalid timestamp".into()));
    };
    let age: Duration = chrono::Utc::now() - timestamp;
    if age > Duration::seconds(OTP_TTL_SEC) {
      return Err(AuthError::BadRequest("OTP expired"));
    }

    Ok(())
  } else {
    return Err(AuthError::BadRequest("invalid user"))
  }
}

pub fn generate_totp_secret() -> String {
  let mut key = [0u8; 20]; // 160 bits
  rng().fill_bytes(&mut key);
  base32::encode(base32::Alphabet::Rfc4648 { padding: false }, &key)
}

fn generate_totp_code(secret: &[u8], time: u64) -> Result<String, AuthError> {
  let step = 30;
  let counter = time / step;
  let counter_bytes = counter.to_be_bytes();

  type HmacSha1 = Hmac<Sha1>;
  let mut mac = HmacSha1::new_from_slice(secret)
    .map_err(|_| AuthError::Internal("Invalid HMAC key length".into()))?;
  mac.update(&counter_bytes);
  let result = mac.finalize().into_bytes();

  let offset = (result.last().unwrap() & 0xf) as usize;
  let binary = ((result[offset] & 0x7f) as u32) << 24
    | ((result[offset + 1]) as u32) << 16
    | ((result[offset + 2]) as u32) << 8
    | ((result[offset + 3]) as u32);

  let otp = binary % 1_000_000;
  Ok(format!("{:06}", otp))
}

#[derive(Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct GenerateTOTPResponse {
  pub secret: String,
  pub qr_code_uri: String,
}

#[utoipa::path(
  post,
  path = "/totp/generate",
  tag = "auth",
  responses(
    (status = 200, description = "TOTP secret and QR code URI.", body = GenerateTOTPResponse)
  )
)]
pub async fn generate_totp_handler(
  State(state): State<AppState>,
  user: User,
) -> Result<Response, AuthError> {
  let secret = generate_totp_secret();
  
  // Generate QR code URI for authenticator apps
  let app_name = state
    .access_config(|c| c.server.application_name.clone())
    .unwrap_or_else(|| "TrailBase".to_string());
  let qr_uri = format!(
    "otpauth://totp/{}:{}?secret={}&issuer={}",
    app_name, user.email, secret, app_name
  );
  
  let response = GenerateTOTPResponse {
    secret: secret.clone(),
    qr_code_uri: qr_uri,
  };
  
  Ok(Json(response).into_response())
}

#[derive(Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct ConfirmTOTPRequest {
  pub secret: String,
  pub totp: String,
}

#[utoipa::path(
  post,
  path = "/totp/confirm",
  tag = "auth",
  responses(
    (status = 200, description = "TOTP confirmed", body = ConfirmTOTPRequest)
  )
)]
pub async fn confirm_totp_handler(
  State(state): State<AppState>,
  user: User,
  Json(params): Json<ConfirmTOTPRequest>,
) -> Result<Response, AuthError> {
  let mut db_user = user_by_email(&state, &user.email).await?;
  db_user.totp_secret = Some(params.secret.clone());
  verify_totp_code_for_user(&db_user, &params.totp)?;
  
  const UPDATE_QUERY: &str = formatcp!(
    "UPDATE '{USER_TABLE}' SET totp_secret = $1 WHERE id = $2"
  );
  
  let user_id_bytes = user.uuid.into_bytes().to_vec();
  state
    .user_conn()
    .execute(UPDATE_QUERY, params!(params.secret.clone(), user_id_bytes))
    .await?;
  
  Ok((StatusCode::OK, "TOTP enabled").into_response())
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct DisableTOTPRequest {
  pub totp: String,
}

#[utoipa::path(
  post,
  path = "/totp/disable",
  tag = "auth",
  responses(
    (status = 200, description = "TOTP disabled successfully.", body = DisableTOTPRequest)
  )
)]
pub async fn disable_totp_handler(
  State(state): State<AppState>,
  user: User,
  Json(params): Json<DisableTOTPRequest>,
) -> Result<Response, AuthError> {
  let db_user = user_by_email(&state, &user.email).await?;
  verify_totp_code_for_user(&db_user, &params.totp)?;
  
  const UPDATE_QUERY: &str = formatcp!(
    "UPDATE '{USER_TABLE}' SET totp_secret = $1 WHERE id = $2"
  );
  
  let user_id_bytes = user.uuid.into_bytes().to_vec();
  state
    .user_conn()
    .execute(UPDATE_QUERY, params!(Option::<String>::None, user_id_bytes))
    .await?;
  
  Ok((StatusCode::OK, "TOTP disabled").into_response())
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct VerifyTOTPRequest {
  pub email: String,
  pub totp: String,
  pub password: Option<String>,
  pub otp: Option<String>,
}

#[utoipa::path(
  post,
  path = "/totp/verify",
  tag = "auth",
  request_body = VerifyTOTPRequest,
  responses(
    (status = 200, description = "Auth tokens.", body = LoginResponse)
  )
)]
pub async fn verify_totp_handler(
  State(state): State<AppState>,
  Json(params): Json<VerifyTOTPRequest>,
) -> Result<Response, AuthError> {
  let email = validate_and_normalize_email_address(&params.email)?;
  let db_user = user_by_email(&state, &email).await?;

  if let Some(password) = params.password {
    check_user_password(&db_user, &password, state.demo_mode())?;
  } else if let Some(otp) = params.otp {
    verify_otp_code(&db_user, &otp)?;
  } else {
    return Err(AuthError::BadRequest("missing params"));
  }

  verify_totp_code_for_user(&db_user, &params.totp)?;

  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  let tokens = mint_new_tokens(state.user_conn(), &db_user, auth_token_ttl).await?;
  
  let response = LoginResponse {
    auth_token: state.jwt().encode(&tokens.auth_token_claims)
      .map_err(|err| AuthError::Internal(err.into()))?,
    refresh_token: tokens.refresh_token,
    csrf_token: tokens.auth_token_claims.csrf_token,
  };
  
  Ok(Json(response).into_response())
}

fn verify_totp_code_for_user(user: &DbUser, code: &str) -> Result<(), AuthError> {
  let Some(totp_secret) = &user.totp_secret else {
    return Err(AuthError::BadRequest("TOTP not enabled for this user"));
  };

  let secret_bytes = match base32::decode(base32::Alphabet::Rfc4648 { padding: false }, totp_secret) {
    Some(bytes) => bytes,
    None => return Err(AuthError::Internal("Invalid TOTP secret format".into())),
  };

  let timestamp = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap()
    .as_secs();

  let expected_code = generate_totp_code(&secret_bytes, timestamp)?;
  
  if expected_code == code {
    Ok(())
  } else {
    // Check previous and next time windows for clock skew tolerance
    let prev_code = generate_totp_code(&secret_bytes, timestamp.saturating_sub(30))?;
    let next_code = generate_totp_code(&secret_bytes, timestamp.saturating_add(30))?;
    
    if code == prev_code || code == next_code {
      Ok(())
    } else {
      return Err(AuthError::BadRequest("Invalid TOTP code"));
    }
  }
}