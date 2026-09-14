use axum::extract::{Extension, Form, Path, Query, State};
use axum::http::{self, HeaderName, HeaderValue, StatusCode};
use axum::response::{AppendHeaders, IntoResponse, Redirect, Response};
use chrono::Utc;
use const_format::formatcp;
use oauth2::{AuthorizationCode, PkceCodeVerifier};
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::params;
use utoipa::{IntoParams, ToSchema};

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::create_external_user::{create_user_for_external_provider, user_by_provider_id};
use crate::auth::oauth::ReqwestClient;
use crate::auth::oauth::state::{OAuthStateClaims, ResponseType};
use crate::auth::options::OAuthEntry;
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{SameSite, new_cookie, remove_cookie, validate_redirect};
use crate::config::proto;
use crate::constants::{
  AUTHORIZATION_CODE_TABLE, COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN,
  DEFAULT_AUTHORIZATION_CODE_TTL, VERIFICATION_CODE_LENGTH,
};
use crate::extract::HasRoot;
use crate::rand::random_alphanumeric;

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub struct CallbackQuery {
  /// Authorization code. Absent when the provider reports an `error` instead, e.g. when
  /// the user cancelled the consent dialog (Apple sends `error` + `state` without `code`).
  pub code: Option<String>,
  pub state: String,
  pub error: Option<String>,
}

/// This handler receives the ?code=<>&state=<>, uses it to get an external oauth token, gets the
/// user's information, creates a new local user if needed, and finally mints our own tokens.
#[utoipa::path(
  get,
  path = "/oauth/{provider}/callback",
  tag = "auth",
  params(CallbackQuery),
  responses(
    (status = 200, description = "Redirect.")
  )
)]
pub(crate) async fn callback_from_external_auth_provider_get(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Extension(HasRoot(has_root)): Extension<HasRoot>,
  cookies: Cookies,
  query: Query<CallbackQuery>,
) -> Result<Response, AuthError> {
  return callback_from_external_auth_provider_impl(&state, provider, query.0, has_root, &cookies)
    .await;
}

#[utoipa::path(
  post,
  path = "/oauth/{provider}/callback",
  tag = "auth",
  request_body(content = CallbackQuery, content_type = "application/x-www-form-urlencoded"),
  responses(
    (status = 200, description = "Redirect.")
  )
)]
pub(crate) async fn callback_from_external_auth_provider_post(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Extension(HasRoot(has_root)): Extension<HasRoot>,
  cookies: Cookies,
  request: Form<CallbackQuery>,
) -> Result<Response, AuthError> {
  return callback_from_external_auth_provider_impl(
    &state, provider, request.0, has_root, &cookies,
  )
  .await;
}

async fn callback_from_external_auth_provider_impl(
  state: &AppState,
  provider: String,
  query: CallbackQuery,
  has_root: bool,
  cookies: &Cookies,
) -> Result<Response, AuthError> {
  if let Some(err) = query.error {
    return Err(AuthError::FailedDependency(err.into()));
  }

  let Some(auth_code) = query.code else {
    return Err(AuthError::BadRequest("missing code"));
  };

  let auth_options = state.auth_options();
  let Some(oauth_entry) = auth_options.lookup_oauth_provider(&provider) else {
    return Err(AuthError::OAuthProviderNotFound);
  };

  // Get round-tripped state from cookies, set by prior call to oauth::login.
  let OAuthStateClaims {
    csrf_secret,
    pkce_code_verifier,
    user_pkce_code_challenge,
    response_type,
    redirect_uri,
    exp: _,
  } = state
    .jwt()
    .decode::<OAuthStateClaims>(
      cookies
        .get(COOKIE_OAUTH_STATE)
        .ok_or_else(|| AuthError::BadRequest("missing state"))?
        .value(),
    )
    .map_err(|_err| {
      remove_cookie(cookies, COOKIE_OAUTH_STATE);
      return AuthError::BadRequest("invalid state");
    })?;

  if csrf_secret != query.state {
    remove_cookie(cookies, COOKIE_OAUTH_STATE);
    return Err(AuthError::BadRequest("invalid state"));
  }

  // NOTE: This was already validated in the login-handler, we're just pedantic.
  let redirect_uri = validate_redirect(state, redirect_uri)?;

  return match response_type {
    Some(ResponseType::Code) => {
      callback_from_oauth_provider_using_auth_code_flow(
        state,
        cookies,
        oauth_entry,
        redirect_uri,
        auth_code,
        pkce_code_verifier,
        user_pkce_code_challenge,
      )
      .await
    }
    _ => {
      callback_from_oauth_provider_setting_token_cookies(
        state,
        cookies,
        oauth_entry,
        redirect_uri,
        auth_code,
        pkce_code_verifier,
        has_root,
      )
      .await
    }
  };
}

/// Log users in using external OAuth setting token cookies on success.
async fn callback_from_oauth_provider_setting_token_cookies(
  state: &AppState,
  cookies: &Cookies,
  oauth_entry: &OAuthEntry,
  redirect: Option<String>,
  auth_code: String,
  server_pkce_code_verifier: String,
  has_root: bool,
) -> Result<Response, AuthError> {
  let db_user =
    get_or_create_user(state, oauth_entry, auth_code, server_pkce_code_verifier).await?;

  // Mint user token and start a session.
  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  let FreshTokens {
    auth_token_claims,
    refresh_token,
    ..
  } = mint_new_tokens(
    state.session_conn(),
    &db_user,
    &auth_token_ttl,
    &refresh_token_ttl,
  )
  .await?;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  cookies.add(new_cookie(
    state,
    COOKIE_AUTH_TOKEN,
    auth_token,
    auth_token_ttl,
    // NOTE: The auth cookie must be same-site=lax in order to be forwarded with the final redirect
    // (e.g. to auth UI), since browsers will still consider this redirection chain as originating
    // from the external oauth provider and thus not to be same site..
    SameSite::Lax,
  ));
  cookies.add(new_cookie(
    state,
    COOKIE_REFRESH_TOKEN,
    refresh_token,
    refresh_token_ttl,
    SameSite::Strict,
  ));

  // NOTE: we're removing the OAUTH_STATE cookie deliberately late in case there are any
  // transient issues, letting users retry.
  remove_cookie(cookies, COOKIE_OAUTH_STATE);

  return if let Some(ref redirect) = redirect {
    Ok((AppendHeaders(NO_REFERER_HEADER), Redirect::to(redirect)).into_response())
  } else if has_root {
    Ok((AppendHeaders(NO_REFERER_HEADER), Redirect::to("/")).into_response())
  } else {
    Ok((StatusCode::OK, "logged in").into_response())
  };
}

/// Creates a random auth code that users can use to subsequently sign in using the
/// `/api/auth/v1/token` endpoint.
///
/// Returns the auth code as a redirect to `<redirect>?auth_code=<code>`.
///
/// This is necessary when clients cannot access cookies, e.g. native client-side apps or apps
/// served from a different origin. For more context, see
/// `crate::auth::api::login::login_with_authorization_code_flow_and_pkce`.
/// Note further that TrailBase requires the use of PKCE when using "authentication code flow".
async fn callback_from_oauth_provider_using_auth_code_flow(
  state: &AppState,
  cookies: &Cookies,
  oauth_entry: &OAuthEntry,
  redirect: Option<String>,
  auth_code: String,
  server_pkce_code_verifier: String,
  user_pkce_code_challenge: Option<String>,
) -> Result<Response, AuthError> {
  let (Some(redirect), Some(user_pkce_code_challenge)) = (redirect, user_pkce_code_challenge)
  else {
    // The OAuth login handler should have already ensured that both are present in the PKCE
    // case. This can only really happen if the state was tempered with.
    remove_cookie(cookies, COOKIE_OAUTH_STATE);
    return Err(AuthError::BadRequest("invalid state"));
  };

  let db_user =
    get_or_create_user(state, oauth_entry, auth_code, server_pkce_code_verifier).await?;

  // For the auth_code flow we generate a random code.
  let authorization_code = random_alphanumeric(VERIFICATION_CODE_LENGTH);

  const QUERY: &str = formatcp!(
    "\
      INSERT INTO \
        '{AUTHORIZATION_CODE_TABLE}' (user, authorization_code, pkce_code_challenge, expires) \
      VALUES \
        ($1, $2, $3, $4)
    "
  );

  let rows_affected = state
    .session_conn()
    .execute(
      QUERY,
      params!(
        db_user.id,
        authorization_code.clone(),
        user_pkce_code_challenge,
        (Utc::now() + DEFAULT_AUTHORIZATION_CODE_TTL).timestamp(),
      ),
    )
    .await?;

  // NOTE: we're removing the OAUTH_STATE cookie deliberately late in case there are any
  // transient issues, letting users retry.
  remove_cookie(cookies, COOKIE_OAUTH_STATE);

  return match rows_affected {
    0 => Err(AuthError::BadRequest("invalid user")),
    1 => Ok(
      (
        AppendHeaders(NO_REFERER_HEADER),
        Redirect::to(&format!("{redirect}?code={authorization_code}")),
      )
        .into_response(),
    ),
    _ => {
      panic!("code challenge update affected multiple users: {rows_affected}");
    }
  };
}

async fn get_or_create_user(
  state: &AppState,
  oauth_entry: &OAuthEntry,
  auth_code: String,
  server_pkce_code_verifier: String,
) -> Result<DbUser, AuthError> {
  let OAuthEntry {
    provider,
    client: oauth_client,
    ..
  } = oauth_entry;

  let http_client = reqwest::ClientBuilder::new()
    // Following redirects might set us up for server-side request forgery (SSRF).
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(|err| AuthError::Internal(err.into()))?;

  let token_response = oauth_client
    .exchange_code(AuthorizationCode::new(auth_code))
    .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
    .request_async(&ReqwestClient(&http_client))
    .await
    .or_else(|err| match err {
      oauth2::RequestTokenError::Parse(path, resp) => provider.parse_token_response(&path, &resp),
      err => Err(AuthError::FailedDependency(err.into())),
    })?;

  // Call provider's USER_INFO endpoint with the tokens acquired above.
  let oauth_user = provider.get_user(&http_client, &token_response).await?;
  if !oauth_user.verified {
    return Err(AuthError::BadRequest("External OAuth user unverified"));
  }

  // Look-up user in local DB to decide whether to create a new one.
  if let Some(existing_user) = user_by_provider_id(
    state.user_conn(),
    oauth_user.provider_id,
    oauth_user.provider_user_id.clone(),
  )
  .await?
  {
    // If user already exists in the local DB, simply return it.
    //
    // TODO: We should probably update the local user if got out of sync, e.g. email changed with
    // external provider.
    return Ok(existing_user);
  };

  let user_identifier = state
    .access_config(|c| c.auth.user_identifier)
    .and_then(|ui| ui.try_into().ok())
    .unwrap_or(proto::UserIdentifier::Undefined);

  // Otherwise, create a new user and return that.
  let db_user =
    create_user_for_external_provider(state.user_conn(), user_identifier, oauth_user).await?;

  // This should never happen. We only ever create a new local user here for verified users above.
  if !db_user.unverified_email.is_none() {
    return Err(AuthError::Internal(
      "OAuth users are expected to be verified".into(),
    ));
  }

  return Ok(db_user);
}

// Unset the "Referer" on final redirect to not leak a user's provider to an arbitrary redirect
// target.
const NO_REFERER_HEADER: [(HeaderName, HeaderValue); 1] = [(
  http::header::REFERRER_POLICY,
  HeaderValue::from_static("no-referrer"),
)];
