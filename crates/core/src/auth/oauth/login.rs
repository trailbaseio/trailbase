use axum::extract::{Path, Query, State};
use axum::response::Redirect;
use chrono::Duration;
use oauth2::{CsrfToken, PkceCodeChallenge, Scope};
use tower_cookies::Cookies;
use tower_cookies::cookie::SameSite;

use crate::AppState;
use crate::auth::AuthError;
use crate::auth::login_params::{LoginInputParams, LoginParams, build_and_validate_input_params};
use crate::auth::oauth::state::{OAuthStateClaims, ResponseType};
use crate::auth::options::OAuthEntry;
use crate::auth::util::{new_cookie_same_site, secure_tls_only};
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

  // Some providers, e.g. Apple, respond to the authorization request with an auto-submitting
  // form POSTing the response to our callback (`response_mode=form_post`) rather than
  // redirecting with query parameters.
  let form_post_response_mode = provider.uses_form_post_response_mode();

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

  // NOTE: we need cookie to be included when redirected back from oauth provider, thus
  // `same_site` can at most be `Lax`. For providers responding with `form_post`, the
  // response is a cross-site POST navigation and `SameSite=Lax` cookies are not attached
  // to those, so we have to fall back to `SameSite=None` (which browsers only honor
  // together with `Secure`, i.e. HTTPS).
  let state_cookie_same_site = if form_post_response_mode {
    SameSite::None
  } else {
    SameSite::Lax
  };

  if form_post_response_mode && !secure_tls_only(&state) {
    log::warn!(
      "OAuth provider '{}' responds with `response_mode=form_post`, but the site \
       isn't served over HTTPS: browsers drop `SameSite=None` cookies without the `Secure` \
       attribute and the OAuth flow will fail at the callback",
      provider.name()
    );
  }

  cookies.add(new_cookie_same_site(
    &state,
    COOKIE_OAUTH_STATE,
    // Encoding as JWT token for tamper proofing. This doesn't encrypt anything but merely adds a
    // signature. None of the state handed to the user needs to be hidden from the user.
    state
      .jwt()
      .encode(&oauth_state)
      .map_err(|err| AuthError::Internal(err.into()))?,
    Duration::minutes(5),
    state_cookie_same_site,
  ));

  Ok(Redirect::to(authorize_url.as_str()))
}
