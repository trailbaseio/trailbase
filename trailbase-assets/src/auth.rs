use askama::Template;

#[derive(Template)]
#[template(path = "login/index.html")]
pub struct LoginTemplate<'a> {
  pub state: String,
  pub alert: &'a str,
  pub enable_registration: bool,
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
