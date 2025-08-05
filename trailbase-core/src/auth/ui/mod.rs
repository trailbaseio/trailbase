use askama::Template;
use axum::Router;
use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::get;
use reqwest::StatusCode;
use serde::Deserialize;
use trailbase_assets::AssetService;
use trailbase_assets::auth::{
  ChangeEmailTemplate, ChangePasswordTemplate, LoginTemplate, OAuthProvider, RegisterTemplate,
  ResetPasswordRequestTemplate, ResetPasswordUpdateTemplate, hidden_input, redirect_uri,
};

use crate::AppState;
use crate::auth::User;

pub(crate) const LOGIN_UI: &str = "/_/auth/login";
pub(crate) const REGISTER_USER_UI: &str = "/_/auth/register";
pub(crate) const PROFILE_UI: &str = "/_/auth/profile";

#[derive(Debug, Default, Deserialize)]
pub struct LoginQuery {
  redirect_uri: Option<String>,
  response_type: Option<String>,
  pkce_code_challenge: Option<String>,
  alert: Option<String>,
}

async fn ui_login_handler(
  State(state): State<AppState>,
  Query(query): Query<LoginQuery>,
  user: Option<User>,
) -> Response {
  if query.redirect_uri.is_none() && user.is_some() {
    // Already logged in. Only redirect to profile-page if no explicit other redirect is provided.
    // For example, if we're already logged in the browser but want to sign-in with the browser
    // from an app, we still have to go through the motions of signing in.
    //
    // QUESTION: Too much magic, just remove?
    return Redirect::to(PROFILE_UI).into_response();
  }

  let oauth_providers: Vec<_> = state
    .auth_options()
    .list_oauth_providers()
    .into_iter()
    .map(|p| OAuthProvider {
      img_name: match p.name.as_str() {
        "discord" | "facebook" | "gitlab" | "google" | "microsoft" => {
          format!("{name}.svg", name = p.name)
        }
        _ => "oidc.svg".to_string(),
      },
      name: p.name,
      display_name: p.display_name,
    })
    .collect();

  let form_state = indoc::formatdoc!(
    r#"
    {redirect_uri}
    {response_type}
    {pkce_code_challenge}
    "#,
    redirect_uri = hidden_input("redirect_uri", query.redirect_uri.as_ref()),
    response_type = hidden_input("response_type", query.response_type.as_ref()),
    pkce_code_challenge = hidden_input("pkce_code_challenge", query.pkce_code_challenge.as_ref()),
  );

  let oauth_query_params: Vec<(&str, &str)> = [
    query
      .redirect_uri
      .as_ref()
      .map(|r| ("redirect_uri", r.as_str())),
    query
      .response_type
      .as_ref()
      .map(|r| ("response_type", r.as_str())),
    query
      .pkce_code_challenge
      .as_ref()
      .map(|r| ("pkce_code_challenge", r.as_str())),
  ]
  .into_iter()
  .flatten()
  .collect();

  let html = LoginTemplate {
    state: form_state,
    alert: query.alert.as_deref().unwrap_or_default(),
    enable_registration: !state.access_config(|c| c.auth.disable_password_auth.unwrap_or(false)),
    oauth_providers: &oauth_providers,
    oauth_query_params: &oauth_query_params,
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct RegisterQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_register_handler(Query(query): Query<RegisterQuery>) -> Response {
  let html = RegisterTemplate {
    state: redirect_uri(query.redirect_uri.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordRequestQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_request_handler(
  Query(query): Query<ResetPasswordRequestQuery>,
) -> Response {
  let html = ResetPasswordRequestTemplate {
    state: redirect_uri(query.redirect_uri.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ResetPasswordUpdateQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_reset_password_update_handler(
  Query(query): Query<ResetPasswordUpdateQuery>,
) -> Response {
  let html = ResetPasswordUpdateTemplate {
    state: redirect_uri(query.redirect_uri.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangePasswordQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_change_password_handler(Query(query): Query<ChangePasswordQuery>) -> Response {
  let html = ChangePasswordTemplate {
    state: redirect_uri(query.redirect_uri.as_ref()),
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct ChangeEmailQuery {
  redirect_uri: Option<String>,
  alert: Option<String>,
}

async fn ui_change_email_handler(Query(query): Query<ChangeEmailQuery>, user: User) -> Response {
  let form_state = indoc::formatdoc!(
    r#"
    {redirect_uri}
    {csrf_token}
  "#,
    redirect_uri = hidden_input("redirect_uri", query.redirect_uri.as_ref()),
    csrf_token = hidden_input("csrf_token", Some(&user.csrf_token)),
  );

  let html = ChangeEmailTemplate {
    state: form_state,
    alert: query.alert.as_deref().unwrap_or_default(),
  }
  .render();

  return match html {
    Ok(html) => Html(html).into_response(),
    Err(err) => (
      StatusCode::INTERNAL_SERVER_ERROR,
      format!("failed to render template: {err}"),
    )
      .into_response(),
  };
}

#[derive(Debug, Default, Deserialize)]
pub struct LogoutQuery {
  redirect_uri: Option<String>,
}

async fn ui_logout_handler(Query(query): Query<LogoutQuery>) -> Redirect {
  if let Some(redirect_uri) = query.redirect_uri {
    return Redirect::to(&format!("/api/auth/v1/logout?redirect_uri={redirect_uri}"));
  }
  return Redirect::to("/api/auth/v1/logout");
}

/// HTML endpoints of core auth functionality.
pub(crate) fn auth_ui_router() -> Router<AppState> {
  return Router::new()
    .route(LOGIN_UI, get(ui_login_handler))
    .route("/_/auth/logout", get(ui_logout_handler))
    .route(REGISTER_USER_UI, get(ui_register_handler))
    .route(
      "/_/auth/reset_password/request",
      get(ui_reset_password_request_handler),
    )
    .route(
      "/_/auth/reset_password/update",
      get(ui_reset_password_update_handler),
    )
    .route("/_/auth/change_password", get(ui_change_password_handler))
    .route("/_/auth/change_email", get(ui_change_email_handler))
    // Static assets for auth UI .
    .nest_service(
      "/_/auth/",
      AssetService::<trailbase_assets::AuthAssets>::with_parameters(
        // We want as little magic as possible. The only /_/auth/subpath that isn't SSR, is
        // profile, so we when hitting /profile or /profile, we want actually want to serve
        // the static profile/index.html.
        Some(Box::new(|path| {
          if path == "profile" {
            Some(format!("{path}/index.html"))
          } else {
            None
          }
        })),
        None,
      ),
    );
}

#[cfg(test)]
mod tests {
  use axum::extract::{Query, State};
  use regex::Regex;
  use std::borrow::Cow;
  use std::collections::HashMap;

  use super::*;
  use crate::app_state::{AppState, TestStateOptions, test_state};
  use crate::auth::oauth::providers::test::TestOAuthProvider;
  use crate::config::proto::{Config, OAuthProviderConfig, OAuthProviderId};
  use crate::constants::AUTH_API_PATH;

  async fn render_html(state: &AppState, query: LoginQuery) -> String {
    let login_response = ui_login_handler(State(state.clone()), Query(query), None).await;

    let body_bytes = axum::body::to_bytes(login_response.into_body(), usize::MAX)
      .await
      .unwrap();
    return String::from_utf8(body_bytes.to_vec()).unwrap();
  }

  #[tokio::test]
  async fn test_ui_login_template() {
    let site_url = "https://test.org";
    let state = test_state(Some(TestStateOptions {
      config: Some({
        let mut config = Config::new_with_custom_defaults();
        config.server.site_url = Some(site_url.to_string());
        config.auth.oauth_providers = [(
          TestOAuthProvider::NAME.to_string(),
          OAuthProviderConfig {
            client_id: Some("test_client_id".to_string()),
            client_secret: Some("test_client_secret".to_string()),
            provider_id: Some(OAuthProviderId::Test as i32),
            ..Default::default()
          },
        )]
        .into();
        config
      }),
      ..Default::default()
    }))
    .await
    .unwrap();

    // NOTE: The build flow will strip newlines from the html/astro template.
    let form_action_re = Regex::new(r#"<form.*action="(.*?)".*?>"#).unwrap();
    let oauth_provider_re = Regex::new(&format!(
      r#"<a.*?href="(/{AUTH_API_PATH}/oauth/.*?)".*?>.*?</a>"#
    ))
    .unwrap();

    {
      // Parameters: empty/default
      let body = render_html(
        &state,
        LoginQuery {
          redirect_uri: None,
          response_type: None,
          pkce_code_challenge: None,
          alert: None,
        },
      )
      .await;

      let form_captures = form_action_re.captures(&body).expect(&body);
      let form_action = form_captures.get(1).unwrap();
      assert_eq!(format!("/{AUTH_API_PATH}/login"), form_action.as_str());

      // Make sure the auth provider is in there.
      let oauth_captures = oauth_provider_re.captures(&body).expect(&body);
      let oauth_provider = oauth_captures.get(1).unwrap();
      assert_eq!(
        format!("/{AUTH_API_PATH}/oauth/{}/login", TestOAuthProvider::NAME),
        oauth_provider.as_str()
      );
    }

    {
      // Parameters: all login parameters
      let redirect_uri = format!("{site_url}/login-success-welcome");
      let response_type = "code";
      let pkce_code_challenge = "challenge";

      let body = render_html(
        &state,
        LoginQuery {
          redirect_uri: Some(redirect_uri.clone()),
          response_type: Some(response_type.to_string()),
          pkce_code_challenge: Some(pkce_code_challenge.to_string()),
          alert: None,
        },
      )
      .await;

      // NOTE: the base action remains the same. For password-login state parameters are
      // passed via hidden form state rather than query params.
      let captures = form_action_re.captures(&body).expect(&body);
      let form_action = captures.get(1).unwrap();
      assert_eq!(format!("/{AUTH_API_PATH}/login"), form_action.as_str());

      assert!(body.contains(&hidden_input("redirect_uri", Some(&redirect_uri))));
      assert!(body.contains(&hidden_input("response_type", Some(response_type))));
      assert!(body.contains(&hidden_input(
        "pkce_code_challenge",
        Some(pkce_code_challenge)
      )));

      // Whereas, OAuth login doesn't receive a form and thus query params instead.
      let oauth_captures = oauth_provider_re.captures(&body).expect(&body);
      let oauth_provider = oauth_captures.get(1).unwrap().as_str();
      assert!(
        oauth_provider.starts_with(&format!(
          "/{AUTH_API_PATH}/oauth/{}/login?",
          TestOAuthProvider::NAME
        )),
        "{oauth_provider}"
      );

      let url = url::Url::parse(&format!("{site_url}/{oauth_provider}")).unwrap();
      let query_params: HashMap<Cow<'_, str>, Cow<'_, str>> = url.query_pairs().collect();

      assert_eq!(
        query_params.get("redirect_uri").map(|s| &**s),
        Some(redirect_uri.as_str()),
        "href: {oauth_provider}"
      );
      assert_eq!(
        query_params.get("response_type").map(|s| &**s),
        Some(response_type),
        "href: {oauth_provider}"
      );
      assert_eq!(
        query_params.get("pkce_code_challenge").map(|s| &**s),
        Some(pkce_code_challenge),
        "href: {oauth_provider}"
      );
    }
  }
}
