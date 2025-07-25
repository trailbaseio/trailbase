use axum::{
  extract::{Json, Query, State},
  response::{IntoResponse, Redirect, Response},
};
use base64::prelude::*;
use chrono::Duration;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::password::check_user_password;
use crate::auth::tokens::mint_new_tokens;
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
use crate::util::urlencode;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct LoginQuery {
  pub redirect_to: Option<String>,
}

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
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

/// Log in users by email and password.
#[utoipa::path(
  post,
  path = "/login",
  tag= "auth",
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
  let (request, is_json) = match either_request {
    Either::Json(req) => (req, true),
    Either::Form(req) => (req, false),
    Either::Multipart(req, _) => (req, false),
  };

  let redirect = validate_redirects(
    &state,
    query.redirect_to.as_deref(),
    request.redirect_to.as_deref(),
  )?;

  let code_response_requested: bool = request.response_type.as_ref().is_some_and(|t| t == "code");
  if !code_response_requested {
    // The simple, non-PKCE case.
    return login_without_pkce(&state, &cookies, request, redirect, is_json).await;
  }

  // The PKCE code-path.
  return login_with_pkce(&state, &cookies, request, redirect).await;
}

/// Log users in with (email, password). On success return tokens (json-case) or set cookies and
/// redirect.
///
/// This is the simple case, i.e. a user browses directly to `/_/auth/login` and logs in with their
/// credentials. Client-side applications (mobile, desktop, SPAs, ...) should use PKCE (see below)
/// to avoid man-in-the-middle attacks through malicious apps on the system.
async fn login_without_pkce(
  state: &AppState,
  cookies: &Cookies,
  request: LoginRequest,
  redirect: Option<String>,
  is_json: bool,
) -> Result<Response, AuthError> {
  // Check credentials.
  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  return match login_with_password(state, &normalized_email, &request.password, auth_token_ttl)
    .await
  {
    Ok(response) if is_json => Ok(Json(response.into_login_response()).into_response()),
    Ok(response) => {
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

      Ok(
        Redirect::to(redirect.as_deref().unwrap_or_else(|| {
          if state.public_dir().is_some() {
            "/"
          } else {
            "/_/auth/profile"
          }
        }))
        .into_response(),
      )
    }
    Err(err) if is_json => Err(err),
    Err(err) => Ok(auth_error_to_response(err, cookies, redirect)),
  };
}

/// Log users in with (email, password). On success redirect users to a client-provided url
/// including a secret (completely random) passed as `?code={auth_code}` query parameter.
/// Requires the user to provide a client-generated "PKCE code challenge".
///
/// Subsequently, clients can complete the login by visiting the `/api/auth/v1/token` endpoint
/// providing both, the `auth_code` from above and the client's "PKCE code verifier".
///
/// Using PKCE prevents against man-in-the-middle attacks by malicious apps, e.g. a webview or
/// browser, interepting the user's tokens upon sign-in. Instead, the client app can fetch the
/// tokens itself given the `auth_code` and the "PKCE code verified", which only it knows.
///
/// An example using the two-step PKCE login can be found in `/examples/blog/flutter`.
async fn login_with_pkce(
  state: &AppState,
  cookies: &Cookies,
  request: LoginRequest,
  redirect: Option<String>,
) -> Result<Response, AuthError> {
  // Note that unlike in the non-PKCE-case, we ignore `is_json` here and always respond with a
  // redirect, as opposed to sending the *auth code* as JSON. Therefore, a valid client-provided
  // redirect is required.
  let Some(redirect) = redirect else {
    // Our own auth UI uses form-submissions. There could be cases where a custom auth UI submits
    // credentials using client-side JS + JSON. Even then responding with a redirect to
    // `{redirect_to}?code={auth_code}` is probably the right thing to do.
    //
    // Ultimately we need to get the *auth code* to the client app, typically via a custom URI
    // scheme the app has registered. Otherwise, the custom client-side auth UI would have to do a
    // local redirect. There could be use-cases where the client-side JS wants to communicate the
    // auth code back to the client application with something other than a custom URI scheme?
    return Err(AuthError::BadRequest("missing 'redirect_to'"));
  };

  // Validate required client-provided PKCE-code-challenge.
  //
  // The challenge is `BASE64_URL_SAFE_NO_PAD.encode(sha256(random(length=32..96)))`. Is there
  // more validation we can or should do?
  let pkce_code_challenge = request.pkce_code_challenge.ok_or_else(|| {
    return AuthError::BadRequest("missing 'pkce_code_challenge'");
  })?;
  if BASE64_URL_SAFE_NO_PAD.decode(&pkce_code_challenge).is_err() {
    return Err(AuthError::BadRequest("invalid 'pkce_code_challenge'"));
  }

  // Check credentials.
  let normalized_email = validate_and_normalize_email_address(&request.email)?;

  if let Err(err) = check_credentials(state, &normalized_email, &request.password).await {
    return Ok(auth_error_to_response(err, cookies, Some(redirect)));
  }

  // We generate a random code for the auth_code flow.
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
    1 => Ok(Redirect::to(&format!("{redirect}?code={authorization_code}")).into_response()),
    _ => {
      panic!("code challenge update affected multiple users: {rows_affected}");
    }
  };
}

fn auth_error_to_response(err: AuthError, cookies: &Cookies, redirect: Option<String>) -> Response {
  let err_response: Response = err.into_response();
  let status = err_response.status();
  if !status.is_client_error() {
    return err_response;
  }

  // We also want to unset existing cookies.
  remove_cookie(cookies, COOKIE_AUTH_TOKEN);
  remove_cookie(cookies, COOKIE_REFRESH_TOKEN);

  let url = format!(
    "/_/auth/login?alert={msg}&{redirect_to}",
    msg = urlencode(&format!("Login Failed: {status}")),
    redirect_to = redirect.map_or_else(
      || "".to_string(),
      |r| format!("redirect_to={}", urlencode(&r))
    ),
  );

  // QUESTION: Should we return a different redirect type (e.g. temporary or permenant) in the
  // error case?
  return Redirect::to(&url).into_response();
}

#[derive(Debug)]
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

pub async fn check_credentials(
  state: &AppState,
  normalized_email: &str,
  password: &str,
) -> Result<(), AuthError> {
  let db_user: DbUser = user_by_email(state, normalized_email).await.map_err(|_| {
    // Don't leak if user wasn't found or password was wrong.
    return AuthError::Unauthorized;
  })?;

  // Validate password.
  check_user_password(&db_user, password, state.demo_mode())?;

  return Ok(());
}

/// Given valid credentials, logs in the user by minting new tokens and therefore also creating a
/// new sessions.
pub(crate) async fn login_with_password(
  state: &AppState,
  normalized_email: &str,
  password: &str,
  auth_token_ttl: Duration,
) -> Result<NewTokens, AuthError> {
  let db_user: DbUser = user_by_email(state, normalized_email).await.map_err(|_| {
    // Don't leak if user wasn't found or password was wrong.
    return AuthError::Unauthorized;
  })?;

  // Validate password.
  check_user_password(&db_user, password, state.demo_mode())?;

  let user_id = db_user.uuid();

  let tokens = mint_new_tokens(state.user_conn(), &db_user, auth_token_ttl).await?;

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
