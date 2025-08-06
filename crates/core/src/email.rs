use lettre::address::AddressError;
use lettre::message::{Body, Mailbox, Message, header::ContentType};
use lettre::transport::smtp;
use lettre::{AsyncSendmailTransport, AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use log::*;
use minijinja::{Environment, context};
use std::sync::Arc;
use thiserror::Error;

use crate::AppState;
use crate::config::proto::{Config, EmailTemplate};

#[derive(Debug, Error)]
pub enum EmailError {
  #[error("Email address error: {0}")]
  Address(#[from] AddressError),
  #[error("Missing error: {0}")]
  Missing(&'static str),
  #[error("Senda error: {0}")]
  Send(#[from] lettre::error::Error),
  #[error("SMTP error: {0}")]
  Smtp(#[from] lettre::transport::smtp::Error),
  #[error("Sendmail error: {0}")]
  Sendmail(#[from] lettre::transport::sendmail::Error),
  #[error("Template error: {0}")]
  Template(#[from] minijinja::Error),
}

pub struct Email {
  mailer: Mailer,

  from: Mailbox,
  to: Mailbox,

  subject: String,
  body: String,
}

impl Email {
  pub fn new(
    state: &AppState,
    to: &str,
    subject: String,
    body: String,
  ) -> Result<Self, EmailError> {
    return Self::new_internal(state, to.parse()?, subject, body);
  }

  fn new_internal(
    state: &AppState,
    to: Mailbox,
    subject: String,
    body: String,
  ) -> Result<Self, EmailError> {
    return Ok(Self {
      mailer: state.mailer(),
      from: get_sender(state)?,
      to,
      subject,
      body,
    });
  }

  pub async fn send(&self) -> Result<(), EmailError> {
    let email = Message::builder()
      .to(self.to.clone())
      .from(self.from.clone())
      .subject(self.subject.clone())
      .header(ContentType::TEXT_HTML)
      .body(Body::new(self.body.clone()))?;

    match self.mailer {
      Mailer::Smtp(ref mailer) => {
        mailer.send(email).await?;
      }
      Mailer::Local(ref mailer) => {
        mailer.send(email).await?;
      }
    };

    return Ok(());
  }

  pub(crate) fn verification_email(
    state: &AppState,
    email: &str,
    email_verification_code: &str,
  ) -> Result<Self, EmailError> {
    let to: Mailbox = email.parse()?;

    let (server_config, template) =
      state.access_config(|c| (c.server.clone(), c.email.user_verification_template.clone()));

    let (subject_template, body_template) = match template {
      Some(EmailTemplate {
        subject: Some(subject),
        body: Some(body),
      }) => (subject, body),
      _ => {
        debug!("Falling back to default email verification email");
        (
          defaults::EMAIL_VALIDATION_SUBJECT.to_string(),
          defaults::EMAIL_VALIDATION_BODY.to_string(),
        )
      }
    };

    let site_url = get_site_url(state);
    let verification_url = site_url
      .join(&format!(
        "/api/auth/v1/verify_email/confirm/{email_verification_code}"
      ))
      .map_err(|_err| EmailError::Missing("email verification URL"))?
      .to_string();

    let env = Environment::empty();
    let subject = env
      .template_from_named_str("subject", &subject_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        EMAIL => email,
      })?;
    let body = env
      .template_from_named_str("body", &body_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        VERIFICATION_URL => verification_url,
        SITE_URL => site_url,
        CODE => email_verification_code,
        EMAIL => email,
      })?;

    return Email::new_internal(state, to, subject, body);
  }

  pub(crate) fn change_email_address_email(
    state: &AppState,
    email: &str,
    email_verification_code: &str,
  ) -> Result<Self, EmailError> {
    let to: Mailbox = email.parse()?;
    let (server_config, template) =
      state.access_config(|c| (c.server.clone(), c.email.change_email_template.clone()));

    let (subject_template, body_template) = match template {
      Some(EmailTemplate {
        subject: Some(subject),
        body: Some(body),
      }) => (subject, body),
      _ => {
        debug!("Falling back to default change email template");
        (
          defaults::CHANGE_EMAIL_SUBJECT.to_string(),
          defaults::CHANGE_EMAIL_BODY.to_string(),
        )
      }
    };

    let site_url = get_site_url(state);
    let verification_url = site_url
      .join(&format!(
        "/api/auth/v1/change_email/confirm/{email_verification_code}"
      ))
      .map_err(|_err| EmailError::Missing("change email confirmation URL"))?
      .to_string();

    let env = Environment::empty();
    let subject = env
      .template_from_named_str("subject", &subject_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        EMAIL => email,
      })?;
    let body = env
      .template_from_named_str("body", &body_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        VERIFICATION_URL => verification_url,
        SITE_URL => site_url,
        CODE => email_verification_code,
        EMAIL => email,
      })?;

    return Email::new_internal(state, to, subject, body);
  }

  pub(crate) fn password_reset_email(
    state: &AppState,
    email: &str,
    password_reset_code: &str,
  ) -> Result<Self, EmailError> {
    let to: Mailbox = email.parse()?;
    let (server_config, template) =
      state.access_config(|c| (c.server.clone(), c.email.password_reset_template.clone()));

    let (subject_template, body_template) = match template {
      Some(EmailTemplate {
        subject: Some(subject),
        body: Some(body),
      }) => (subject, body),
      _ => {
        debug!("Falling back to default reset password email");
        (
          defaults::PASSWORD_RESET_SUBJECT.to_string(),
          defaults::PASSWORD_RESET_BODY.to_string(),
        )
      }
    };

    let site_url = get_site_url(state);
    let verification_url = site_url
      .join(&format!(
        "/api/auth/v1/reset_password/update/{password_reset_code}"
      ))
      .map_err(|_err| EmailError::Missing("password reset URL"))?
      .to_string();

    let env = Environment::empty();
    let subject = env
      .template_from_named_str("subject", &subject_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        EMAIL => email,
      })?;
    let body = env
      .template_from_named_str("body", &body_template)?
      .render(context! {
        APP_NAME => server_config.application_name,
        VERIFICATION_URL => verification_url,
        SITE_URL => site_url,
        CODE => password_reset_code,
        EMAIL => email,
      })?;

    return Email::new_internal(state, to, subject, body);
  }
}

fn get_sender(state: &AppState) -> Result<Mailbox, EmailError> {
  let (sender_address, sender_name) =
    state.access_config(|c| (c.email.sender_address.clone(), c.email.sender_name.clone()));

  let address = sender_address.unwrap_or_else(|| fallback_sender(&state.site_url()));

  if let Some(ref name) = sender_name {
    return Ok(format!("{name} <{address}>").parse::<Mailbox>()?);
  }
  return Ok(address.parse::<Mailbox>()?);
}

fn fallback_sender(site_url: &Option<url::Url>) -> String {
  if let Some(host) = site_url.as_ref().and_then(|u| u.host()) {
    return format!("noreply@{host}");
  }

  warn!(
    "No 'site_url' configured, falling back to sender 'noreply@localhost'. This may be ok for development environments but otherwise will result in your emails being filtered."
  );

  return "noreply@localhost".to_string();
}

#[derive(Clone)]
pub(crate) enum Mailer {
  Smtp(Arc<dyn AsyncTransport<Ok = smtp::response::Response, Error = smtp::Error> + Send + Sync>),
  Local(Arc<AsyncSendmailTransport<Tokio1Executor>>),
}

impl Mailer {
  fn new_smtp(host: String, port: u16, user: String, pass: String) -> Result<Mailer, EmailError> {
    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?
      .port(port)
      .credentials(smtp::authentication::Credentials::new(user, pass))
      .build();
    return Ok(Mailer::Smtp(Arc::new(mailer)));
  }

  fn new_local() -> Mailer {
    return Mailer::Local(Arc::new(AsyncSendmailTransport::<Tokio1Executor>::new()));
  }

  pub(crate) fn new_from_config(config: &Config) -> Mailer {
    let smtp_from_config = || -> Result<Mailer, EmailError> {
      let email = &config.email;
      let host = email
        .smtp_host
        .to_owned()
        .ok_or(EmailError::Missing("SMTP host"))?;
      let port = email
        .smtp_port
        .map(|port| port as u16)
        .ok_or(EmailError::Missing("SMTP port"))?;
      let user = email
        .smtp_username
        .to_owned()
        .ok_or(EmailError::Missing("SMTP username"))?;
      let pass = email
        .smtp_password
        .to_owned()
        .ok_or(EmailError::Missing("SMTP password"))?;

      Self::new_smtp(host, port, user, pass)
    };

    if let Ok(mailer) = smtp_from_config() {
      return mailer;
    }

    return Self::new_local();
  }
}

fn get_site_url(state: &AppState) -> url::Url {
  return match *state.site_url() {
    Some(ref site_url) => site_url.clone(),
    None => {
      // TODO: We should forward the actual server address.
      warn!(
        "No 'site_url' configured, falling back to 'http://localhost:4000'. This may be ok for development but will result in invalid auth links otherwise."
      );

      url::Url::parse("http://localhost:4000").expect("invariant")
    }
  };
}

pub(crate) mod defaults {
  use crate::config::proto::EmailTemplate;
  use indoc::indoc;

  pub const EMAIL_VALIDATION_SUBJECT: &str = "Verify your Email Address for {{ APP_NAME }}";
  pub const EMAIL_VALIDATION_BODY: &str = indoc! {r#"
        <html>
          <body>
            <h1>Welcome {{ EMAIL }}</h1>

            <p>
              Thanks for joining {{ APP_NAME }}.
            </p>

            <p>
              To be able to log in, first validate your email by clicking the link below.
            </p>

            <a class="btn" href="{{ VERIFICATION_URL }}">
              {{ VERIFICATION_URL }}
            </a>
          </body>
        </html>"#};

  pub fn email_validation_email() -> EmailTemplate {
    return EmailTemplate {
      subject: Some(EMAIL_VALIDATION_SUBJECT.into()),
      body: Some(EMAIL_VALIDATION_BODY.into()),
    };
  }

  pub const PASSWORD_RESET_SUBJECT: &str = "Reset your Password for {{ APP_NAME }}";
  pub const PASSWORD_RESET_BODY: &str = indoc! {r#"
        <html>
          <body>
            <h1>Password Reset</h1>

            <p>
              Click the link below to reset your password.
            </p>

            <a class="btn" href="{{ VERIFICATION_URL }}">
              {{ VERIFICATION_URL }}
            </a>
          </body>
        </html>"#};

  pub fn password_reset_email() -> EmailTemplate {
    return EmailTemplate {
      subject: Some(PASSWORD_RESET_SUBJECT.into()),
      body: Some(PASSWORD_RESET_BODY.into()),
    };
  }

  pub const CHANGE_EMAIL_SUBJECT: &str = "Change your Email Address for {{ APP_NAME }}";
  pub const CHANGE_EMAIL_BODY: &str = indoc! {r#"
        <html>
          <body>
            <h1>Change E-Mail Address</h1>

            <p>
              Click the link below to verify your new E-mail address:
            </p>

            <a class="btn" href="{{ VERIFICATION_URL }}">
              {{ VERIFICATION_URL }}
            </a>
          </body>
        </html>"#};

  pub fn change_email_address_email() -> EmailTemplate {
    return EmailTemplate {
      subject: Some(CHANGE_EMAIL_SUBJECT.into()),
      body: Some(CHANGE_EMAIL_BODY.into()),
    };
  }
}

#[cfg(test)]
pub mod testing {
  use lettre::AsyncTransport;
  use lettre::address::Envelope;
  use lettre::transport::smtp::response::{Category, Code, Detail, Response, Severity};
  use parking_lot::Mutex;
  use std::sync::Arc;

  use super::*;
  use crate::app_state::test_state;

  #[derive(Clone)]
  pub struct TestAsyncSmtpTransport {
    response: Response,
    log: Arc<Mutex<Vec<(Envelope, String)>>>,
  }

  impl TestAsyncSmtpTransport {
    pub fn new() -> TestAsyncSmtpTransport {
      let code = Code::new(
        Severity::PositiveCompletion,
        Category::Information,
        Detail::Zero,
      );

      return TestAsyncSmtpTransport {
        response: Response::new(code, vec![]),
        log: Arc::new(Mutex::new(Vec::new())),
      };
    }

    pub fn get_logs(&self) -> Vec<(Envelope, String)> {
      return self.log.lock().clone();
    }
  }

  #[async_trait::async_trait]
  impl AsyncTransport for TestAsyncSmtpTransport {
    type Ok = lettre::transport::smtp::response::Response;
    type Error = lettre::transport::smtp::Error;

    async fn send_raw(&self, envelope: &Envelope, email: &[u8]) -> Result<Self::Ok, Self::Error> {
      self
        .log
        .lock()
        .push((envelope.clone(), String::from_utf8_lossy(email).into()));

      return Ok(self.response.clone());
    }
  }

  #[tokio::test]
  async fn test_template_rendering() {
    let state = test_state(None).await.unwrap();

    let code = "verification_code0123.";
    {
      let email = Email::verification_email(&state, "foo@bar.org", code).unwrap();
      assert_eq!(email.subject, "Verify your Email Address for TrailBase");
      assert!(email.body.contains("Welcome foo@bar.org"));
      assert!(email.body.contains(&format!(
        "https://test.org/api/auth/v1/verify_email/confirm/{code}"
      )));
    }

    {
      let email = Email::change_email_address_email(&state, "foo@bar.org", code).unwrap();
      assert_eq!(email.subject, "Change your Email Address for TrailBase");
      assert!(email.body.contains(&format!(
        "https://test.org/api/auth/v1/change_email/confirm/{code}"
      )));
    }

    {
      let email = Email::password_reset_email(&state, "foo@bar.org", code).unwrap();
      assert_eq!(email.subject, "Reset your Password for TrailBase");
      assert!(email.body.contains(&format!(
        "https://test.org/api/auth/v1/reset_password/update/{code}"
      )));
    }
  }

  #[test]
  fn test_fallback_sender() {
    let url = Some(url::Url::parse("https://test.org").unwrap());
    let sender = fallback_sender(&url);
    assert_eq!("noreply@test.org", sender);
  }
}
