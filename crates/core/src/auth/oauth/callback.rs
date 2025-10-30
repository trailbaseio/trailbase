use axum::{
  extract::{Path, Query, State},
  response::Redirect,
};
use lazy_static::lazy_static;
use oauth2::{AuthorizationCode, PkceCodeVerifier};
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::{named_params, params};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::PROFILE_UI;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::provider::TokenResponse;
use crate::auth::oauth::providers::OAuthProviderType;
use crate::auth::oauth::state::{OAuthState, ResponseType};
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{get_user_by_id, new_cookie, remove_cookie, validate_redirect};
use crate::config::proto::OAuthProviderId;
use crate::constants::{
  COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN, USER_TABLE, VERIFICATION_CODE_LENGTH,
};
use crate::rand::generate_random_string;

#[derive(Debug, Deserialize, IntoParams)]
pub struct AuthQuery {
  pub code: String,
  pub state: String,
}

/// This handler receives the ?code=<>&state=<>, uses it to get an external oauth token, gets the
/// user's information, creates a new local user if needed, and finally mints our own tokens.
#[utoipa::path(
  get,
  path = "/{provider}/callback",
  tag = "oauth",
  params(AuthQuery),
  responses(
    (status = 200, description = "Redirect.")
  )
)]
pub(crate) async fn callback_from_external_auth_provider(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Query(query): Query<AuthQuery>,
  cookies: Cookies,
) -> Result<Redirect, AuthError> {
  let auth_options = state.auth_options();
  let Some(provider) = auth_options.lookup_oauth_provider(&provider) else {
    return Err(AuthError::OAuthProviderNotFound);
  };

  // Get round-tripped state from cookies, set by prior call to oauth::login.
  let OAuthState {
    csrf_secret,
    pkce_code_verifier,
    user_pkce_code_challenge,
    response_type,
    redirect_uri,
    ..
  } = state
    .jwt()
    .decode::<OAuthState>(
      cookies
        .get(COOKIE_OAUTH_STATE)
        .ok_or_else(|| AuthError::BadRequest("missing state"))?
        .value(),
    )
    .map_err(|_err| {
      remove_cookie(&cookies, COOKIE_OAUTH_STATE);
      return AuthError::BadRequest("invalid state");
    })?;

  if csrf_secret != query.state {
    remove_cookie(&cookies, COOKIE_OAUTH_STATE);
    return Err(AuthError::BadRequest("invalid state"));
  }

  // NOTE: This was already validated in the login-handler, we're just pedantic.
  validate_redirect(&state, redirect_uri.as_deref())?;

  return match response_type {
    Some(ResponseType::Code) => {
      callback_from_oauth_provider_using_auth_code_flow(
        &state,
        &cookies,
        provider,
        redirect_uri,
        query.code,
        pkce_code_verifier,
        user_pkce_code_challenge,
      )
      .await
    }
    _ => {
      callback_from_oauth_provider_setting_token_cookies(
        &state,
        &cookies,
        provider,
        redirect_uri,
        query.code,
        pkce_code_verifier,
      )
      .await
    }
  };
}

/// Log users in using external OAuth setting token cookies on success.
async fn callback_from_oauth_provider_setting_token_cookies(
  state: &AppState,
  cookies: &Cookies,
  provider: &OAuthProviderType,
  redirect: Option<String>,
  auth_code: String,
  server_pkce_code_verifier: String,
) -> Result<Redirect, AuthError> {
  let db_user = get_or_create_user(state, provider, auth_code, server_pkce_code_verifier).await?;

  // Mint user token and start a session.
  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());

  let FreshTokens {
    auth_token_claims,
    refresh_token,
    ..
  } = mint_new_tokens(state.user_conn(), &db_user, auth_token_ttl).await?;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  cookies.add(new_cookie(
    COOKIE_AUTH_TOKEN,
    auth_token,
    auth_token_ttl,
    state.dev_mode(),
  ));
  cookies.add(new_cookie(
    COOKIE_REFRESH_TOKEN,
    refresh_token,
    refresh_token_ttl,
    state.dev_mode(),
  ));

  // NOTE: we're removing the OAUTH_STATE cookie deliberately late in case there are any
  // transient issues, letting users retry.
  remove_cookie(cookies, COOKIE_OAUTH_STATE);

  return Ok(Redirect::to(match (redirect, state.public_dir()) {
    (Some(ref redirect), _) => redirect,
    (None, Some(_)) => "/",
    (None, None) => PROFILE_UI,
  }));
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
  provider: &OAuthProviderType,
  redirect: Option<String>,
  auth_code: String,
  server_pkce_code_verifier: String,
  user_pkce_code_challenge: Option<String>,
) -> Result<Redirect, AuthError> {
  let (Some(redirect), Some(user_pkce_code_challenge)) = (redirect, user_pkce_code_challenge)
  else {
    // The OAuth login handler should have already ensured that both are present in the PKCE
    // case. This can only really happen if the state was tempered with.
    remove_cookie(cookies, COOKIE_OAUTH_STATE);
    return Err(AuthError::BadRequest("invalid state"));
  };

  let db_user = get_or_create_user(state, provider, auth_code, server_pkce_code_verifier).await?;

  // For the auth_code flow we generate a random code.
  let authorization_code = generate_random_string(VERIFICATION_CODE_LENGTH);

  lazy_static! {
    pub static ref QUERY: String = format!(
      r#"
        UPDATE
          '{USER_TABLE}'
        SET
          authorization_code = :authorization_code,
          authorization_code_sent_at = UNIXEPOCH(),
          pkce_code_challenge = :pkce_code_challenge
        WHERE
          id = :user_id
      "#
    );
  }

  let rows_affected = state
    .user_conn()
    .execute(
      &*QUERY,
      named_params! {
        ":authorization_code": authorization_code.clone(),
        ":pkce_code_challenge": user_pkce_code_challenge,
        ":user_id": db_user.id,
      },
    )
    .await?;

  // NOTE: we're removing the OAUTH_STATE cookie deliberately late in case there are any
  // transient issues, letting users retry.
  remove_cookie(cookies, COOKIE_OAUTH_STATE);

  return match rows_affected {
    0 => Err(AuthError::BadRequest("invalid user")),
    1 => Ok(Redirect::to(&format!(
      "{redirect}?code={authorization_code}"
    ))),
    _ => {
      panic!("code challenge update affected multiple users: {rows_affected}");
    }
  };
}

async fn get_or_create_user(
  state: &AppState,
  provider: &OAuthProviderType,
  auth_code: String,
  server_pkce_code_verifier: String,
) -> Result<DbUser, AuthError> {
  let http_client = reqwest::ClientBuilder::new()
    // Following redirects might set us up for server-side request forgery (SSRF).
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(|err| AuthError::Internal(err.into()))?;

  // Call provider's TOKEN endpoint to exchange auth_code + (server_)pkce_code_verifier
  // for tokens. We then use these tokens to call the USER_INFO endpoint below to get
  // information, such as email address, to create a local TrailBase user.
  let token_response: TokenResponse = provider
    .oauth_client(state)?
    .exchange_code(AuthorizationCode::new(auth_code))
    .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
    .request_async(&http_client)
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  // Call provider's USER_INFO endpoint with the tokens acquired above.
  let oauth_user = provider.get_user(&token_response).await.and_then(|user| {
    if !user.verified {
      return Err(AuthError::BadRequest("External OAuth user unverified"));
    }
    return Ok(user);
  })?;

  let existing_user = user_by_provider_id(
    state.user_conn(),
    oauth_user.provider_id,
    &oauth_user.provider_user_id,
  )
  .await
  .ok();

  return match existing_user {
    Some(existing_user) => Ok(existing_user),
    None => {
      // NOTE: We could combine the INSERT + SELECT.
      let id = create_user_for_external_provider(state.user_conn(), &oauth_user).await?;
      let db_user = get_user_by_id(state.user_conn(), &id).await?;

      // We should have only ever created the local user, if the external user was verified.
      assert!(db_user.verified);
      if !db_user.verified {
        return Err(AuthError::Internal(
          "OAuth users are expected to be verified".into(),
        ));
      }

      Ok(db_user)
    }
  };
}

async fn create_user_for_external_provider(
  conn: &trailbase_sqlite::Connection,
  user: &OAuthUser,
) -> Result<Uuid, AuthError> {
  if !user.verified {
    return Err(AuthError::Unauthorized);
  }

  lazy_static! {
    static ref QUERY: String = format!(
      r#"
        INSERT INTO {USER_TABLE} (
          provider_id, provider_user_id, verified, email, provider_avatar_url
        ) VALUES (
          :provider_id, :provider_user_id, :verified, :email, :avatar
        ) RETURNING id
      "#
    );
  }

  let id: Uuid = conn
    .write_query_value(
      &*QUERY,
      named_params! {
          ":provider_id": user.provider_id as i64,
          ":provider_user_id": user.provider_user_id.clone(),
          ":verified": user.verified as i64,
          ":email": user.email.clone(),
          ":avatar": user.avatar.clone(),
      },
    )
    .await?
    .ok_or_else(|| AuthError::Internal("query should return".into()))?;

  return Ok(id);
}

async fn user_by_provider_id(
  conn: &trailbase_sqlite::Connection,
  provider_id: OAuthProviderId,
  provider_user_id: &str,
) -> Result<DbUser, AuthError> {
  lazy_static! {
    static ref QUERY: String =
      format!("SELECT * FROM '{USER_TABLE}' WHERE provider_id = $1 AND provider_user_id = $2");
  };

  return conn
    .read_query_value::<DbUser>(
      &*QUERY,
      params!(provider_id as i64, provider_user_id.to_string()),
    )
    .await?
    .ok_or_else(|| AuthError::NotFound);
}
