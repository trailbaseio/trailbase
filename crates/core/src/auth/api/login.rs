use axum::{
  extract::{Json, Query, State},
  response::{IntoResponse, Redirect, Response},
};
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use tower_cookies::Cookies;
use trailbase_sqlite::named_params;
use ts_rs::TS;
use utoipa::ToSchema;

use crate::auth::AuthError;
use crate::auth::login_params::{
  LoginInputParams, LoginParams, ResponseType, build_and_validate_input_params,
};
use crate::auth::password::check_user_password;
use crate::auth::user::DbUser;
use crate::auth::util::{
  new_cookie, remove_cookie, user_by_email, validate_and_normalize_email_address,
};
use crate::auth::{LOGIN_UI, PROFILE_UI};
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_REFRESH_TOKEN, DEFAULT_MFA_TOKEN_TTL, USER_TABLE,
  VERIFICATION_CODE_LENGTH,
};
use crate::extract::Either;
use crate::rand::generate_random_string;
use crate::util::urlencode;
use crate::{app_state::AppState, auth::jwt::PendingAuthTokenClaims};

#[derive(Debug, Default, Deserialize, TS, ToSchema)]
#[ts(export)]
pub struct LoginRequest {
  pub email: String,
  pub password: String,

  // Mirror of LoginInputParams.
  pub redirect_uri: Option<String>,
  pub mfa_redirect_uri: Option<String>,
  pub response_type: Option<ResponseType>,
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
  params(LoginInputParams),
  request_body = LoginRequest,
  responses(
    (status = 200, description = "Auth & refresh tokens.", body = LoginResponse)
  )
)]
pub(crate) async fn login_handler(
  State(state): State<AppState>,
  Query(query_login_input): Query<LoginInputParams>,
  cookies: Cookies,
  either_request: Either<LoginRequest>,
) -> Result<Response, AuthError> {
  let (
    LoginRequest {
      email,
      password,
      response_type,
      pkce_code_challenge,
      redirect_uri,
      mfa_redirect_uri,
    },
    is_json,
  ) = match either_request {
    Either::Json(req) => (req, true),
    Either::Form(req) => (req, false),
    Either::Multipart(req, _) => (req, false),
  };

  // Validate input params.
  let params = build_and_validate_input_params(
    &state,
    // NOTE: Merge form and query input but prioritize explicit query parameters over hidden form
    // inputs etc.
    query_login_input.merge(LoginInputParams {
      redirect_uri: redirect_uri.clone(),
      mfa_redirect_uri: mfa_redirect_uri.clone(),
      response_type,
      pkce_code_challenge,
    }),
  )?;

  // Check credentials.
  let normalized_email = validate_and_normalize_email_address(&email)?;
  let db_user: DbUser = user_by_email(&state, &normalized_email)
    .await
    .map_err(|_| {
      // Don't leak if user wasn't found or password was wrong.
      return AuthError::Unauthorized;
    })?;

  // Check password and rate limits attempts.
  if let Err(err) = check_user_password(&db_user, &password, state.demo_mode()) {
    if is_json {
      return Err(err);
    }
    return Ok(auth_error_to_response(
      err,
      &cookies,
      redirect_uri.as_deref(),
    ));
  }

  // Does the user require multi-factor auth (MFA)?
  if db_user.totp_secret.is_some() {
    let Some(mfa_redirect) = mfa_redirect_uri else {
      return Err(AuthError::BadRequest("TOTP redirect required"));
    };

    let token = state
      .jwt()
      .encode(&PendingAuthTokenClaims::new(
        db_user.uuid(),
        DEFAULT_MFA_TOKEN_TTL,
      ))
      .map_err(|err| AuthError::Internal(err.into()))?;

    return Ok(Redirect::to(&format!("{mfa_redirect}?mfa_token={token}")).into_response());
  }

  // Otherwise build auth token or authorization code responses.
  return match params {
    // Auth-token flow.
    LoginParams::Password { redirect_uri } => {
      build_auth_token_flow_response(&state, &db_user, &cookies, redirect_uri, is_json).await
    }
    // Authorization-code flow.
    LoginParams::AuthorizationCodeFlowWithPkce {
      redirect_uri,
      pkce_code_challenge,
    } => {
      build_authorization_code_flow_and_pkce_response(
        &state,
        &db_user,
        redirect_uri,
        pkce_code_challenge,
      )
      .await
    }
  };
}

/// Log users in with (email, password). On success return tokens (json-case) or set cookies and
/// redirect.
///
/// This is the simplest case, i.e. a client calls `/_/auth/login` directly with user credentials
/// to log in and retrieve tokens. This works for well for password-based login and custom auth
/// UIs.
/// This is also what the built-in auth UI uses by default to pass tokens as cookies. However, the
/// cookie-based approach only works for web-apps hosted with the same origin like the admin UI.
/// Otherwise, the cookies will be inaccessible and the "authentication code" flow below is needed
/// to get the tokens to your app.
async fn build_auth_token_flow_response(
  state: &AppState,
  db_user: &DbUser,
  cookies: &Cookies,
  redirect: Option<String>,
  is_json: bool,
) -> Result<Response, AuthError> {
  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  let build_new_tokens = async || {
    let tokens =
      crate::auth::tokens::mint_new_tokens(state.user_conn(), db_user, auth_token_ttl).await?;

    return Ok(LoginResponse {
      auth_token: state
        .jwt()
        .encode(&tokens.auth_token_claims)
        .map_err(|err| AuthError::Internal(err.into()))?,
      refresh_token: tokens.refresh_token,
      csrf_token: tokens.auth_token_claims.csrf_token,
    });
  };

  return match build_new_tokens().await {
    Ok(response) if is_json => Ok(Json(response).into_response()),
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
        Redirect::to(match (redirect, state.public_dir()) {
          (Some(ref redirect), _) => redirect,
          (None, Some(_)) => "/",
          (None, None) => PROFILE_UI,
        })
        .into_response(),
      )
    }
    Err(err) if is_json => Err(err),
    Err(err) => Ok(auth_error_to_response(err, cookies, redirect.as_deref())),
  };
}

/// Password-based login using "authentication code flow" and required Proof-Key-for-Key-Exchange
/// (PKCE) (RFC7636).
///
/// Whenever a web auth UI is used (TrailBase's or an external OAuth provider's) the question
/// becomes how to get the tokens to the application? - especially if it's not a web app or it is
/// served from a different origin...
/// The "authentication code flow" answers this question with: upon successful sign-in, redirect
/// the user to a registered callback address: `<callback>?code=<auth_code>`.
/// Native client-side applications or SPAs can achieve this by registering a callback with a
/// custom scheme, e.g. "my-app://callback" and awaiting the `auth_code`.
///
/// Tokens can be lengthy, thus sending them as a query parameter is brittle (and interceptable).
/// The "authentication code flow" therefore uses an intermediary `auth_code`, which can
/// subsequently be exchanged (typically together with another secret) by calling an token exchange
/// HTTP endpoint from the client directly (here `/api/auth/v1/token`).
///
/// PKCE is an elegant protocol to establish the additional secret that is send alongside the auth
/// code w/o baking a secret into the client app (i.e. not really a secret), while protecting
/// against man-in-the-middle attacks by a malicious or infected browser/Webview. TrailBase
/// therefore **requires** PKCE when using "authentication code flow".
///
/// An example using the two-step "authentication code flow" with PKCE can be found in
/// `/examples/blog/flutter`.
async fn build_authorization_code_flow_and_pkce_response(
  state: &AppState,
  db_user: &DbUser,
  redirect: String,
  pkce_code_challenge: String,
) -> Result<Response, AuthError> {
  // We generate a random code for the auth_code flow.
  let authorization_code = generate_random_string(VERIFICATION_CODE_LENGTH);

  const QUERY: &str = formatcp!(
    "\
      UPDATE \
        '{USER_TABLE}' \
      SET \
        authorization_code = :authorization_code, \
        authorization_code_sent_at = UNIXEPOCH(), \
        pkce_code_challenge = :pkce_code_challenge \
      WHERE \
        email = :email \
    "
  );

  let rows_affected = state
    .user_conn()
    .execute(
      QUERY,
      named_params! {
        ":authorization_code": authorization_code.clone(),
        ":pkce_code_challenge": pkce_code_challenge,
        ":email": db_user.email.clone(),
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

fn auth_error_to_response(err: AuthError, cookies: &Cookies, redirect: Option<&str>) -> Response {
  let err_response: Response = err.into_response();
  let status = err_response.status();
  if !status.is_client_error() {
    return err_response;
  }

  // We also want to unset existing cookies.
  remove_cookie(cookies, COOKIE_AUTH_TOKEN);
  remove_cookie(cookies, COOKIE_REFRESH_TOKEN);

  // QUESTION: Should we return a different redirect type (e.g. temporary or permanent) in the
  // error case?
  let msg = urlencode(&format!("Login Failed: {status}"));
  return if let Some(redirect) = redirect {
    Redirect::to(&format!("{LOGIN_UI}?alert={msg}&redirect_uri={redirect}")).into_response()
  } else {
    Redirect::to(&format!("{LOGIN_UI}?alert={msg}")).into_response()
  };
}
