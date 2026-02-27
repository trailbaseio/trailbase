use axum::{
  extract::{Path, Query, State},
  response::Redirect,
};
use chrono::Duration;
use oauth2::{CsrfToken, PkceCodeChallenge, Scope};
use tower_cookies::Cookies;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::login_params::{LoginInputParams, LoginParams, build_and_validate_input_params};
use crate::auth::oauth::state::{OAuthStateClaims, ResponseType};
use crate::auth::util::new_cookie_opts;
use crate::constants::COOKIE_OAUTH_STATE;

/// Log in via external OAuth provider.
#[utoipa::path(
  get,
  path = "/{provider}/login",
  tag = "oauth",
  params(LoginInputParams),
  responses(
    (status = 200, description = "Redirect.")
  )
)]
pub(crate) async fn login_with_external_auth_provider(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  Query(login_input_query): Query<LoginInputParams>,
  cookies: Cookies,
) -> Result<Redirect, AuthError> {
  let auth_options = state.auth_options();
  let Some(provider) = auth_options.lookup_oauth_provider(&provider) else {
    return Err(AuthError::OAuthProviderNotFound);
  };
  let login_params = build_and_validate_input_params(&state, login_input_query)?;

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

  let oauth_state = match login_params {
    LoginParams::Password { redirect_uri } => OAuthStateClaims {
      // Set short-lived CSRF and PkceCodeVerifier cookies for the callback.
      exp: (chrono::Utc::now() + Duration::seconds(5 * 60)).timestamp(),
      csrf_secret: csrf_state.secret().to_string(),
      pkce_code_verifier: server_pkce_code_verifier.secret().to_string(),
      redirect_uri,
      response_type: None,
      user_pkce_code_challenge: None,
    },
    LoginParams::AuthorizationCodeFlowWithPkce {
      redirect_uri,
      pkce_code_challenge,
    } => OAuthStateClaims {
      // Set short-lived CSRF and PkceCodeVerifier cookies for the callback.
      exp: (chrono::Utc::now() + Duration::seconds(5 * 60)).timestamp(),
      csrf_secret: csrf_state.secret().to_string(),
      pkce_code_verifier: server_pkce_code_verifier.secret().to_string(),
      user_pkce_code_challenge: Some(pkce_code_challenge),
      response_type: Some(ResponseType::Code),
      redirect_uri: Some(redirect_uri),
    },
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
