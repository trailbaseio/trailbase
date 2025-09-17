use askama::Template;
use askama::filters::Safe;
use itertools::Itertools;
use rust_embed::RustEmbed;
use serde::Deserialize;

#[derive(RustEmbed, Clone)]
#[folder = "../assets/js/auth/dist/"]
pub struct AuthAssets;

#[derive(Deserialize)]
pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
  pub img_name: String,
}

#[derive(Deserialize)]
pub struct AuthConfig {
  pub disable_password_auth: bool,
  pub oauth_providers: Vec<OAuthProvider>,
}

/// Render a slice of tuples into an unescpaed query string.
///
/// Careful, this should only be used on safe, static inputs only. We want HTML
/// escaping by default, it's safer for dynamic inputs like `alert`.
pub fn render_safe_query_params(params: &[(&str, &str)]) -> Safe<String> {
  if params.is_empty() {
    return Safe(String::new());
  }
  return Safe(format!(
    "?{}",
    params.iter().map(|(k, v)| format!("{k}={v}")).join("&")
  ));
}

#[derive(Template)]
#[template(path = "login/index.html")]
pub struct LoginTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
  pub enable_registration: bool,
  pub oauth_providers: &'a [OAuthProvider],
  pub oauth_query_params: &'a [(&'a str, &'a str)],
}

#[derive(Template)]
#[template(path = "register/index.html")]
pub struct RegisterTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
}

#[derive(Template)]
#[template(path = "reset_password/request/index.html")]
pub struct ResetPasswordRequestTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
}

#[derive(Template)]
#[template(path = "reset_password/update/index.html")]
pub struct ResetPasswordUpdateTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
}

#[derive(Template)]
#[template(path = "change_password/index.html")]
pub struct ChangePasswordTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
}

#[derive(Template)]
#[template(path = "change_email/index.html")]
pub struct ChangeEmailTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
}

pub fn hidden_input<T>(name: &str, value: Option<T>) -> String
where
  T: AsRef<str>,
{
  if let Some(value) = value {
    return format!(
      r#"<input name="{name}" type="hidden" value="{value}" />"#,
      value = value.as_ref()
    );
  }
  return "".to_string();
}

pub fn redirect_uri<T>(redirect_uri: Option<T>) -> String
where
  T: AsRef<str>,
{
  return hidden_input("redirect_uri", redirect_uri);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_login_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";
    let redirect_uri = "http://localhost:42";

    let template = LoginTemplate {
      state: state.clone(),
      alert,
      enable_registration: true,
      oauth_providers: &[],
      oauth_query_params: &[("redirect_uri", redirect_uri)],
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(&redirect_uri), "{template}"); // Not escaped.
    // Missing because no oauth provider given.
    assert!(!template.contains("foo=bar"), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.

    let oauth_template = LoginTemplate {
      state: state.clone(),
      alert: "",
      enable_registration: false,
      oauth_providers: &[OAuthProvider {
        name: "name".to_string(),
        display_name: "Fancy Name".to_string(),
        img_name: "oidc".to_string(),
      }],
      oauth_query_params: &[("redirect_uri", redirect_uri), ("foo", "bar")],
    }
    .render()
    .unwrap();

    assert!(oauth_template.contains(&state), "{template}"); // Not escaped.
    assert!(oauth_template.contains(&redirect_uri), "{template}"); // Not escaped.
    assert!(oauth_template.contains("foo=bar"), "{template}"); // Not escaped.
  }

  #[test]
  fn test_register_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";

    let template = RegisterTemplate {
      state: state.clone(),
      alert,
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.
  }

  #[test]
  fn test_reset_password_request_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";

    let template = ResetPasswordRequestTemplate {
      state: state.clone(),
      alert,
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.
  }

  #[test]
  fn test_reset_password_update_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";

    let template = ResetPasswordUpdateTemplate {
      state: state.clone(),
      alert,
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.
  }

  #[test]
  fn test_change_password_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";

    let template = ChangePasswordTemplate {
      state: state.clone(),
      alert,
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.
  }

  #[test]
  fn test_change_email_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";

    let template = ChangeEmailTemplate {
      state: state.clone(),
      alert,
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(alert), "{template}"); // Is escaped.
  }
}
