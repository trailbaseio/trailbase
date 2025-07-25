use axum::{
  extract::{Path, Query, State},
  response::Redirect,
};
use lazy_static::lazy_static;
use oauth2::{AuthorizationCode, PkceCodeVerifier, StandardTokenResponse, TokenResponse};
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::{named_params, params};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::providers::OAuthProviderType;
use crate::auth::oauth::state::{OAuthState, ResponseType};
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{get_user_by_id, new_cookie, remove_cookie, validate_redirects};
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
  let oauth_state = {
    let oauth_state_cookie = cookies
      .get(COOKIE_OAUTH_STATE)
      .ok_or_else(|| AuthError::BadRequest("missing state"))?
      .value()
      .to_owned();

    remove_cookie(&cookies, COOKIE_OAUTH_STATE);

    state
      .jwt()
      .decode::<OAuthState>(&oauth_state_cookie)
      .map_err(|_err| AuthError::BadRequest("invalid state"))
      .and_then(|state| {
        if state.csrf_secret != query.state {
          return Err(AuthError::BadRequest("invalid state"));
        }
        return Ok(state);
      })?
  };

  let redirect = validate_redirects(&state, oauth_state.redirect_to.as_deref(), None)?;

  return match oauth_state.response_type {
    Some(ResponseType::Code) => {
      callback_from_external_auth_provider_with_pkce(
        &state,
        provider,
        redirect,
        query.code,
        oauth_state.pkce_code_verifier,
        oauth_state.user_pkce_code_challenge,
      )
      .await
    }
    _ => {
      callback_from_external_auth_provider_without_pkce(
        &state,
        &cookies,
        provider,
        redirect,
        query.code,
        oauth_state.pkce_code_verifier,
      )
      .await
    }
  };
}

/// Log users in using external OAuth.
///
/// This is the simple case, i.e. a user browses directly to `/_/auth/login` and logs in with their
/// credentials. Client-side applications (mobile, desktop, SPAs, ...) should use PKCE (see below)
/// to avoid man-in-the-middle attacks through malicious apps on the system.
async fn callback_from_external_auth_provider_without_pkce(
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

  return Ok(Redirect::to(redirect.as_deref().unwrap_or_else(|| {
    if state.public_dir().is_some() {
      "/"
    } else {
      "/_/auth/profile"
    }
  })));
}

/// Log users in using external OAuth. On success redirect users to a client-provided url including
/// a secret (completely random) passed as `?code={auth_code}` query parameter. Requires the user
/// to provide a client-generated "PKCE code challenge".
///
/// TODO: Needs test coverage.
async fn callback_from_external_auth_provider_with_pkce(
  state: &AppState,
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

  // Exchange code for token.
  let token_response: StandardTokenResponse<_, oauth2::basic::BasicTokenType> = provider
    .oauth_client(state)?
    .exchange_code(AuthorizationCode::new(auth_code))
    .set_pkce_verifier(PkceCodeVerifier::new(server_pkce_code_verifier))
    .request_async(&http_client)
    .await
    .map_err(|err| AuthError::FailedDependency(err.into()))?;

  if *token_response.token_type() != oauth2::basic::BasicTokenType::Bearer {
    return Err(AuthError::Internal(
      format!("Unexpected token type: {:?}", token_response.token_type()).into(),
    ));
  }

  let oauth_user = provider
    .get_user(token_response.access_token().secret().clone())
    .await
    .and_then(|user| {
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
