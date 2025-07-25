use askama::Template;

pub struct OAuthProvider {
  pub name: String,
  pub display_name: String,
  pub img_name: String,
}

#[derive(Template)]
#[template(path = "login/index.html")]
pub struct LoginTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
  pub enable_registration: bool,
  pub oauth_providers: &'a [OAuthProvider],
  pub oauth_query_params: Option<String>,
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

pub fn redirect_to<T>(redirect_to: Option<T>) -> String
where
  T: AsRef<str>,
{
  return hidden_input("redirect_to", redirect_to);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_login_template_escaping() {
    let state = hidden_input("TEST", Some("FOO"));
    let alert = "<><>";
    let redirect_to = "http://localhost:42";

    let template = LoginTemplate {
      state: state.clone(),
      alert,
      enable_registration: true,
      oauth_providers: &[],
      oauth_query_params: Some(format!("redirect_to={redirect_to}&foo=bar")),
    }
    .render()
    .unwrap();

    assert!(template.contains(&state), "{template}"); // Not escaped.
    assert!(!template.contains(&redirect_to), "{template}"); // Not escaped.
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
      oauth_query_params: Some(format!("redirect_to={redirect_to}&foo=bar")),
    }
    .render()
    .unwrap();

    assert!(oauth_template.contains(&state), "{template}"); // Not escaped.
    assert!(oauth_template.contains(&redirect_to), "{template}"); // Not escaped.
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
