use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use chrono::Utc;
use const_format::formatcp;
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::{named_params, params};
use utoipa::IntoParams;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::providers::OAuthProviderType;
use crate::auth::oauth::state::{OAuthStateClaims, ResponseType};
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{
  new_cookie, remove_cookie, validate_and_normalize_username, validate_redirect,
};
use crate::config::proto::{OAuthProviderId, UserIdentifier};
use crate::constants::{
  AUTHORIZATION_CODE_TABLE, COOKIE_AUTH_TOKEN, COOKIE_OAUTH_STATE, COOKIE_REFRESH_TOKEN,
  DEFAULT_AUTHORIZATION_CODE_TTL, USER_TABLE, VERIFICATION_CODE_LENGTH,
};
use crate::rand::random_alphanumeric;

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
) -> Result<Response, AuthError> {
  let auth_options = state.auth_options();
  let Some(provider) = auth_options.lookup_oauth_provider(&provider) else {
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
      remove_cookie(&cookies, COOKIE_OAUTH_STATE);
      return AuthError::BadRequest("invalid state");
    })?;

  if csrf_secret != query.state {
    remove_cookie(&cookies, COOKIE_OAUTH_STATE);
    return Err(AuthError::BadRequest("invalid state"));
  }

  // NOTE: This was already validated in the login-handler, we're just pedantic.
  let redirect_uri = validate_redirect(&state, redirect_uri)?;

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
) -> Result<Response, AuthError> {
  let db_user = get_or_create_user(state, provider, auth_code, server_pkce_code_verifier).await?;

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
  ));
  cookies.add(new_cookie(
    state,
    COOKIE_REFRESH_TOKEN,
    refresh_token,
    refresh_token_ttl,
  ));

  // NOTE: we're removing the OAUTH_STATE cookie deliberately late in case there are any
  // transient issues, letting users retry.
  remove_cookie(cookies, COOKIE_OAUTH_STATE);

  return if let Some(ref redirect) = redirect {
    Ok(Redirect::to(redirect).into_response())
  } else if state.public_dir().is_some() {
    Ok(Redirect::to("/").into_response())
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
  provider: &OAuthProviderType,
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

  let db_user = get_or_create_user(state, provider, auth_code, server_pkce_code_verifier).await?;

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
    1 => Ok(Redirect::to(&format!("{redirect}?code={authorization_code}")).into_response()),
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
  let token_response = provider
    .get_token(state, auth_code, server_pkce_code_verifier)
    .await?;

  // Call provider's USER_INFO endpoint with the tokens acquired above.
  let oauth_user = provider.get_user(&token_response).await?;
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
    .unwrap_or(UserIdentifier::Undefined);

  // Otherwise, create a new user and return that.
  let db_user =
    create_user_for_external_provider(state.user_conn(), user_identifier, oauth_user).await?;

  // This should never happen. We only ever create a new local user here for verified users above.
  if !db_user.verified {
    return Err(AuthError::Internal(
      "OAuth users are expected to be verified".into(),
    ));
  }

  return Ok(db_user);
}

async fn create_user_for_external_provider(
  conn: &trailbase_sqlite::Connection,
  user_identifier: UserIdentifier,
  user: OAuthUser,
) -> Result<DbUser, AuthError> {
  let OAuthUser {
    provider_user_id,
    provider_id,
    email,
    username,
    verified,
    avatar,
  } = user;

  if !verified {
    return Err(AuthError::Unauthorized);
  }

  let mut username: Option<String> = match (user_identifier, username) {
    (UserIdentifier::OnlyEmail | UserIdentifier::Undefined, _) => None,
    (
      UserIdentifier::OnlyUsername
      | UserIdentifier::RequireUsername
      | UserIdentifier::RequireEmailAndUsername,
      username,
    ) => Some(
      username
        .and_then(|u| validate_and_normalize_username(&u).ok())
        .unwrap_or_else(|| {
          // Since we strictly need a username, make one up.
          format!(
            "user{suffix}",
            suffix = crate::rand::random_numeric_and_lowercase(6)
          )
        }),
    ),
    (UserIdentifier::RequireEmail, username) => username,
  };

  if let Some(username) = username.as_mut() {
    // Check availability and potentially append randomness.
    const EXISTS_QUERY: &str =
      formatcp!("SELECT EXISTS(SELECT 1 FROM \"{USER_TABLE}\" WHERE username = $1)");

    // To be pedantic we check for collisions in a loop.
    let mut i = 0;
    while conn
      .read_query_row_get::<bool>(EXISTS_QUERY, params!(username.clone()), 0)
      .await?
      .unwrap_or(false)
      && i < 5
    {
      *username = format!(
        "{username}{suffix}",
        suffix = crate::rand::random_numeric_and_lowercase(6)
      );
      i += 1;
    }

    debug_assert!(validate_and_normalize_username(username).is_ok());
  }

  const QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{USER_TABLE}\" ( \
        provider_id, provider_user_id, verified, email, username, provider_avatar_url \
      ) VALUES ( \
        :provider_id, :provider_user_id, :verified, :email, :username, :avatar \
      ) RETURNING * \
    "
  );

  let db_user: DbUser = conn
    .write_query_value(
      QUERY,
      named_params! {
          ":provider_id": provider_id as i64,
          ":provider_user_id": provider_user_id,
          ":verified": verified as i64,
          ":email": email,
          ":username": username,
          ":avatar": avatar,
      },
    )
    .await?
    .ok_or_else(|| AuthError::Internal("insertion issue".into()))?;

  return Ok(db_user);
}

async fn user_by_provider_id(
  conn: &trailbase_sqlite::Connection,
  provider_id: OAuthProviderId,
  provider_user_id: String,
) -> Result<Option<DbUser>, AuthError> {
  const QUERY: &str =
    formatcp!(r#"SELECT * FROM "{USER_TABLE}" WHERE provider_id = $1 AND provider_user_id = $2"#);

  return Ok(
    conn
      .read_query_value::<DbUser>(QUERY, params!(provider_id as i64, provider_user_id))
      .await?,
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::test_state;

  #[tokio::test]
  async fn test_oauth_create_user() {
    let state = test_state(None).await.unwrap();

    fn user(username: Option<String>) -> OAuthUser {
      let rand = crate::rand::random_numeric_and_lowercase(20);
      return OAuthUser {
        provider_user_id: rand.clone(),
        provider_id: OAuthProviderId::Test,
        email: format!("email_{rand}@test.org"),
        username,
        verified: true,
        avatar: None,
      };
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireEmail,
        user(None),
      )
      .await
      .unwrap();

      assert!(created.username.is_none());
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::OnlyEmail,
        user(Some("test".to_string())),
      )
      .await
      .unwrap();

      assert!(created.username.is_none());
    }

    {
      let created = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(None),
      )
      .await
      .unwrap();

      assert!(created.username.is_some());
    }

    {
      let username = "duplicate".to_string();
      let created0 = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(Some(username.clone())),
      )
      .await
      .unwrap();

      assert!(created0.username.is_some());

      let created1 = create_user_for_external_provider(
        state.user_conn(),
        UserIdentifier::RequireUsername,
        user(Some(username.clone())),
      )
      .await
      .unwrap();

      assert!(created1.username.is_some());
      assert_ne!(created0.username, created1.username);
    }
  }
}
