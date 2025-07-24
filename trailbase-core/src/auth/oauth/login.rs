use axum::{
  extract::{Path, Query, State},
  response::Redirect,
};
use base64::prelude::*;
use chrono::Duration;
use oauth2::{CsrfToken, PkceCodeChallenge, Scope};
use serde::Deserialize;
use tower_cookies::Cookies;
use utoipa::IntoParams;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::oauth::state::{OAuthState, ResponseType};
use crate::auth::util::{new_cookie_opts, validate_redirects};
use crate::constants::COOKIE_OAUTH_STATE;

#[derive(Debug, Default, Deserialize, IntoParams)]
pub(crate) struct LoginQuery {
  pub redirect_to: Option<String>,
  pub response_type: Option<String>,
  pub pkce_code_challenge: Option<String>,
}

/// Log in via external OAuth provider.
#[utoipa::path(
  get,
  path = "/{provider}/login",
  tag = "oauth",
  params(LoginQuery),
  responses(
    (status = 200, description = "Redirect.")
  )
)]
pub(crate) async fn login_with_external_auth_provider(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Query(query): Query<LoginQuery>,
  cookies: Cookies,
) -> Result<Redirect, AuthError> {
  let auth_options = state.auth_options();
  let Some(provider) = auth_options.lookup_oauth_provider(&provider) else {
    return Err(AuthError::OAuthProviderNotFound);
  };
  let redirect = validate_redirects(&state, query.redirect_to.as_deref(), None)?;
  let user_pkce_code_challenge = query.pkce_code_challenge;
  let response_type = if query.response_type.is_some_and(|r| r == "code") {
    Some(ResponseType::Code)
  } else {
    None
  };

  // PKCE code flow requires the client to provide both a valid redirect and `pkce_code_challenge`.
  if response_type == Some(ResponseType::Code) {
    if redirect.is_none() {
      return Err(AuthError::BadRequest("missing 'redirect_to'"));
    }

    if BASE64_URL_SAFE_NO_PAD
      .decode(user_pkce_code_challenge.as_deref().ok_or_else(|| {
        return AuthError::BadRequest("missing 'pkce_code_challenge'");
      })?)
      .is_err()
    {
      return Err(AuthError::BadRequest("invalid 'pkce_code_challenge'"));
    };
  }

  // Also use PKCE between TrailBase and the external auth provider. Is is independent from PKCE
  // between the client and TrailBase.
  let (server_pkce_code_challenge, server_pkce_code_verifier) =
    PkceCodeChallenge::new_random_sha256();

  let (authorize_url, csrf_state) = provider
    .oauth_client(&state)?
    .authorize_url(CsrfToken::new_random)
    .add_scopes(
      provider
        .oauth_scopes()
        .into_iter()
        .map(|s| Scope::new(s.to_string())),
    )
    .set_pkce_challenge(server_pkce_code_challenge)
    .url();

  let oauth_state = OAuthState {
    // Set short-lived CSRF and PkceCodeVerifier cookies for the callback.
    exp: (chrono::Utc::now() + Duration::seconds(5 * 60)).timestamp(),
    csrf_secret: csrf_state.secret().to_string(),
    pkce_code_verifier: server_pkce_code_verifier.secret().to_string(),
    user_pkce_code_challenge,
    response_type,
    redirect_to: redirect,
  };

  cookies.add(new_cookie_opts(
    COOKIE_OAUTH_STATE,
    // Encoding as JWT token for tamper proofing. This doesn't encrypt anything but merely adds a
    // signature. None of the state handed to the user needs to be hidden from the user.
    state
      .jwt()
      .encode(&oauth_state)
      .map_err(|err| AuthError::Internal(err.into()))?,
    Duration::minutes(5),
    state.dev_mode(),
    // We need to include cookies on redirect back from oauth provider.
    /* same_site: */
    false,
  ));

  Ok(Redirect::to(authorize_url.as_str()))
}
