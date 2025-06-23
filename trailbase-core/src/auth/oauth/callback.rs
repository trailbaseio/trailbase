use axum::{
  extract::{Path, Query, State},
  response::Redirect,
};
use chrono::Duration;
use lazy_static::lazy_static;
use oauth2::PkceCodeVerifier;
use oauth2::{AuthorizationCode, StandardTokenResponse, TokenResponse};
use serde::Deserialize;
use tower_cookies::Cookies;
use trailbase_sqlite::{named_params, params};
use utoipa::IntoParams;
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::OAuthUser;
use crate::auth::oauth::state::{OAuthState, ResponseType};
use crate::auth::tokens::{FreshTokens, mint_new_tokens};
use crate::auth::user::DbUser;
use crate::auth::util::{new_cookie, remove_cookie, user_by_id, validate_redirects};
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

  // Get round-tripped state from the users browser.
  let Some(oauth_state) = cookies.get(COOKIE_OAUTH_STATE).and_then(|cookie| {
    // The decoding can fail if the state was tampered with.
    state.jwt().decode::<OAuthState>(cookie.value()).ok()
  }) else {
    return Err(AuthError::BadRequest("missing state"));
  };

  let redirect = validate_redirects(&state, &oauth_state.redirect_to, &None)?;

  if oauth_state.csrf_secret != query.state {
    return Err(AuthError::BadRequest("invalid state"));
  }

  let http_client = reqwest::ClientBuilder::new()
    // Following redirects opens the client up to SSRF vulnerabilities.
    .redirect(reqwest::redirect::Policy::none())
    .build()
    .map_err(|err| AuthError::Internal(err.into()))?;

  let client = provider.oauth_client(&state)?;

  // Exchange code for token.
  let token_response: StandardTokenResponse<_, oauth2::basic::BasicTokenType> = client
    .exchange_code(AuthorizationCode::new(query.code))
    .set_pkce_verifier(PkceCodeVerifier::new(oauth_state.pkce_code_verifier))
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
    .await?;

  if !oauth_user.verified {
    return Err(AuthError::BadRequest("remote oauth user not verified"));
  }

  let conn = state.user_conn();
  let existing_user =
    user_by_provider_id(conn, oauth_user.provider_id, &oauth_user.provider_user_id)
      .await
      .ok();

  let db_user = match existing_user {
    Some(existing_user) => existing_user,
    None => {
      // NOTE: We could combine the INSERT + SELECT.
      let id = create_user_for_external_provider(conn, &oauth_user).await?;
      let db_user = user_by_id(&state, &id).await?;
      assert!(db_user.verified);

      if !db_user.verified {
        return Err(AuthError::Internal(
          "user created from oauth should be verified".into(),
        ));
      }

      db_user
    }
  };

  // Mint user token.
  let (auth_token_ttl, refresh_token_ttl) = state.access_config(|c| c.auth.token_ttls());
  let expires_in = token_response.expires_in().map_or(auth_token_ttl, |exp| {
    Duration::seconds(exp.as_secs() as i64)
  });

  let FreshTokens {
    auth_token_claims,
    refresh_token,
    ..
  } = mint_new_tokens(
    &state,
    db_user.verified,
    db_user.uuid(),
    db_user.email,
    expires_in,
  )
  .await?;

  let auth_token = state
    .jwt()
    .encode(&auth_token_claims)
    .map_err(|err| AuthError::Internal(err.into()))?;

  cookies.add(new_cookie(
    COOKIE_AUTH_TOKEN,
    auth_token,
    expires_in,
    state.dev_mode(),
  ));
  cookies.add(new_cookie(
    COOKIE_REFRESH_TOKEN,
    refresh_token,
    refresh_token_ttl,
    state.dev_mode(),
  ));

  remove_cookie(&cookies, COOKIE_OAUTH_STATE);

  if let Some(response_type) = oauth_state.response_type {
    if response_type == ResponseType::Code {
      if redirect.is_none() {
        return Err(AuthError::BadRequest("missing 'redirect_to'"));
      };

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
            ":pkce_code_challenge": oauth_state.user_pkce_code_challenge,
            ":user_id": db_user.id,
          },
        )
        .await?;

      match rows_affected {
        0 => return Err(AuthError::BadRequest("invalid user")),
        1 => {}
        _ => {
          panic!("code challenge update affected multiple users: {rows_affected}");
        }
      };
    }
  }

  return Ok(Redirect::to(redirect.as_deref().unwrap_or_else(|| {
    if state.public_dir().is_some() {
      "/"
    } else {
      "/_/auth/profile"
    }
  })));
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
