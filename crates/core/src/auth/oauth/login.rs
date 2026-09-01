use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use chrono::Duration;
use oauth2::{CsrfToken, PkceCodeChallenge, Scope};
use tower_cookies::Cookies;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::login_params::{LoginInputParams, LoginParams, build_and_validate_input_params};
use crate::auth::oauth::state::{OAuthStateClaims, ResponseType};
use crate::auth::options::OAuthEntry;
use crate::auth::util::{SameSite, new_cookie};
use crate::config::proto;
use crate::constants::COOKIE_OAUTH_STATE;

/// Log in via external OAuth provider.
#[utoipa::path(
  get,
  path = "/oauth/{provider}/login",
  tag = "auth",
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
  let Some(oauth_entry) = auth_options.lookup_oauth_provider(&provider) else {
    return Err(AuthError::OAuthProviderNotFound);
  };

  let OAuthEntry {
    provider,
    client: oauth_client,
    ..
  } = oauth_entry;

  let login_params = build_and_validate_input_params(&state, login_input_query)?;
  let user_identifier = state
    .access_config(|c| c.auth.user_identifier)
    .and_then(|ui| ui.try_into().ok())
    .unwrap_or(proto::UserIdentifier::Undefined);

  // Also use PKCE between TrailBase and the external auth provider. Is is independent from PKCE
  // between the client and TrailBase.
  let (server_pkce_code_challenge, server_pkce_code_verifier) =
    PkceCodeChallenge::new_random_sha256();

  let (authorize_url, csrf_state) = oauth_client
    .authorize_url(CsrfToken::new_random)
    .add_scopes(
      provider
        .oauth_scopes(user_identifier)
        .into_iter()
        .map(Scope::new),
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

  cookies.add(new_cookie(
    &state,
    COOKIE_OAUTH_STATE,
    // Encoding as JWT token for tamper proofing. This doesn't encrypt anything but merely adds a
    // signature. None of the state handed to the user needs to be hidden from the user.
    state
      .jwt()
      .encode(&oauth_state)
      .map_err(|err| AuthError::Internal(err.into()))?,
    Duration::minutes(5),
    // NOTE: we need cookie to be included when redirected back from oauth provider, thus:
    SameSite::None,
  ));

  Ok(Redirect::to(authorize_url.as_str()))
}
