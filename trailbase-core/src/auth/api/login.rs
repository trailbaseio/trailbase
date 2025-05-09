use axum::{
  Json,
  extract::{Query, State},
  response::{IntoResponse, Redirect, Response},
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::password::check_user_password;
use crate::auth::tokens::{Tokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{
  new_cookie, remove_cookie, user_by_email, validate_and_normalize_email_address,
  validate_redirects,
};
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_REFRESH_TOKEN, USER_TABLE, VERIFICATION_CODE_LENGTH,
};
use crate::extract::Either;
use crate::rand::generate_random_string;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct LoginQuery {
  pub redirect_to: Option<String>,
}

#[derive(Debug, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginRequest {
  pub email: String,
  pub password: String,

  pub redirect_to: Option<String>,
  pub response_type: Option<String>,
  pub pkce_code_challenge: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginResponse {
  pub auth_token: String,
  pub refresh_token: String,
  pub csrf_token: String,
}

/// Logs in user by email and password.
#[utoipa::path(
  post,
  path = "/login",
  params(LoginQuery),
  request_body = LoginRequest,
  responses(
    (status = 200, description = "Auth & refresh tokens.", body = LoginResponse)
  )
)]
pub(crate) async fn login_handler(
  State(state): State<AppState>,
  Query(query): Query<LoginQuery>,
  cookies: Cookies,
  either_request: Either<LoginRequest>,
) -> Result<Response, AuthError> {
  let (request, json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Form(req) => (req, false),
    Either::Multipart(req, _) => (req, false),
  };

  let normalized_email = validate_and_normalize_email_address(&request.email)?;
  let redirect = validate_redirects(&state, &query.redirect_to, &request.redirect_to)?;
  let code_response = request.response_type.as_ref().is_some_and(|t| t == "code");
  let pkce_code_challenge = request.pkce_code_challenge.clone();

  // Check credentials.
  let response_or = login_with_password(&state, &normalized_email, &request.password).await;

  if json {
    return Ok(Json(response_or?.into_login_response()).into_response());
  }

  // Cookie and redirect handling for the non-json case. The assumption is that json login is used
  // by SPAs or mobile applications, which should handle credential passing explicitly. No cookies
  // also removes the risk for any CSRF.
  let response = match response_or {
    Ok(response) => response,
    Err(err) => {
      let err_response: Response = err.into_response();
      let status = err_response.status();
      if status.is_client_error() {
        // We also want to unset existing cookies.
        remove_cookie(&cookies, COOKIE_AUTH_TOKEN);
        remove_cookie(&cookies, COOKIE_REFRESH_TOKEN);

        let url = format!(
          "/_/auth/login?alert={msg}",
          msg = crate::util::urlencode(&format!("Login Failed: {status}"))
        );

        return Ok(Redirect::to(&url).into_response());
      }
      return Ok(err_response);
    }
  };

  if code_response {
    let Some(redirect) = redirect else {
      return Err(AuthError::BadRequest("missing 'redirect_to'"));
    };

    // For the auth_code flow we generate a random code.
    let authorization_code = generate_random_string(VERIFICATION_CODE_LENGTH);

    lazy_static! {
      static ref QUERY: String = indoc::formatdoc!(
        r#"
        UPDATE
          "{USER_TABLE}"
        SET
          authorization_code = :authorization_code,
          authorization_code_sent_at = UNIXEPOCH(),
          pkce_code_challenge = :pkce_code_challenge
        WHERE
          email = :email
      "#
      );
    }

    let rows_affected = state
      .user_conn()
      .execute(
        &*QUERY,
        named_params! {
          ":authorization_code": authorization_code.clone(),
          ":pkce_code_challenge": pkce_code_challenge,
          ":email": normalized_email,
        },
      )
      .await?;

    return match rows_affected {
      0 => Err(AuthError::BadRequest("invalid user")),
      1 => {
        // TODO: could be smarter with merging here.
        let url = format!("{redirect}?code={authorization_code}");
        Ok(Redirect::to(&url).into_response())
      }
      _ => {
        panic!("code challenge update affected multiple users: {rows_affected}");
      }
    };
  }

  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  cookies.add(new_cookie(
    COOKIE_AUTH_TOKEN,
    response.auth_token,
    auth_token_ttl,
    state.dev_mode(),
  ));
  cookies.add(new_cookie(
    COOKIE_REFRESH_TOKEN,
    response.refresh_token,
    refresh_token_ttl,
    state.dev_mode(),
  ));

  return Ok(
    Redirect::to(redirect.as_deref().unwrap_or_else(|| {
      if state.public_dir().is_some() {
        "/"
      } else {
        "/_/auth/profile"
      }
    }))
    .into_response(),
  );
}

#[derive(Debug, Serialize, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginStatusResponse {
  pub auth_token: Option<String>,
  pub refresh_token: Option<String>,
  pub csrf_token: Option<String>,
}

/// Check login status.
#[utoipa::path(
  get,
  path = "/status",
  responses(
    (status = 200, description = "Auth & refresh tokens.", body = LoginStatusResponse)
  )
)]
pub(crate) async fn login_status_handler(
  State(state): State<AppState>,
  tokens: Option<Tokens>,
) -> Result<Json<LoginStatusResponse>, AuthError> {
  let Some(tokens) = tokens else {
    return Ok(Json(LoginStatusResponse {
      auth_token: None,
      refresh_token: None,
      csrf_token: None,
    }));
  };

  let Tokens {
    auth_token_claims,
    refresh_token,
  } = tokens;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(Json(LoginStatusResponse {
    auth_token: Some(auth_token),
    refresh_token,
    csrf_token: Some(auth_token_claims.csrf_token),
  }));
}

pub struct NewTokens {
  pub id: uuid::Uuid,
  pub auth_token: String,
  pub refresh_token: String,
  pub csrf_token: String,
}

impl NewTokens {
  fn into_login_response(self) -> LoginResponse {
    return LoginResponse {
      auth_token: self.auth_token,
      refresh_token: self.refresh_token,
      csrf_token: self.csrf_token,
    };
  }
}

pub async fn login_with_password(
  state: &AppState,
  normalized_email: &str,
  password: &str,
) -> Result<NewTokens, AuthError> {
  let db_user: DbUser = user_by_email(state, normalized_email).await?;

  // Validate password.
  check_user_password(&db_user, password, state.demo_mode())?;

  let (auth_token_ttl, _refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  let user_id = db_user.uuid();

  let tokens = mint_new_tokens(
    state,
    db_user.verified,
    user_id,
    db_user.email,
    auth_token_ttl,
  )
  .await?;

  return Ok(NewTokens {
    id: user_id,
    auth_token: state
      .jwt()
      .encode(&tokens.auth_token_claims)
      .map_err(|err| AuthError::Internal(err.into()))?,
    refresh_token: tokens.refresh_token,
    csrf_token: tokens.auth_token_claims.csrf_token,
  });
}
