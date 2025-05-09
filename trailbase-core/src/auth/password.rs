use argon2::{
  Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
  password_hash::{SaltString, rand_core::OsRng},
};
use lazy_static::lazy_static;
use mini_moka::sync::Cache;

use crate::auth::AuthError;
use crate::auth::user::DbUser;

pub struct PasswordOptions {
  pub min_length: usize,
  pub max_length: usize,
}

impl Default for PasswordOptions {
  fn default() -> Self {
    return PasswordOptions {
      min_length: 8,
      max_length: 128,
    };
  }
}

pub fn validate_password_policy(
  password: &str,
  password_repeat: &str,
  opts: &PasswordOptions,
) -> Result<(), AuthError> {
  if password != password_repeat {
    return Err(AuthError::BadRequest("Passwords don't match"));
  }

  if password.len() < opts.min_length {
    return Err(AuthError::BadRequest("Password too short"));
  }

  if password.len() > opts.max_length {
    return Err(AuthError::BadRequest("Password too long"));
  }

  return Ok(());
}

#[derive(Clone)]
struct FailedAttempt {
  tries: usize,
}

impl Default for FailedAttempt {
  fn default() -> Self {
    return Self { tries: 1 };
  }
}

lazy_static! {
  static ref ARGON2: Argon2<'static> = Argon2::default();
  static ref ATTEMPTS: Cache<String, FailedAttempt> = Cache::builder()
    .time_to_live(std::time::Duration::from_secs(5 * 60))
    .max_capacity(1024)
    .build();
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
  let salt = SaltString::generate(&mut OsRng);
  return Ok(
    ARGON2
      .hash_password(password.as_bytes(), &salt)
      .map_err(|err| {
        // NOTE: Wrapping needed since Argon's error doesn't implement the error trait.
        AuthError::Internal(err.to_string().into())
      })?
      .to_string(),
  );
}

/// Checks the given password against a known user. Will further ensure that the email was verified
/// and rate limit attempts to protect against brute-force attacks.
pub fn verify_password(db_user: &DbUser, password: &str, is_demo: bool) -> Result<(), AuthError> {
  if !db_user.verified {
    return Err(AuthError::Unauthorized);
  }
  let attempts = ATTEMPTS.get(&db_user.email);
  if !is_demo && attempts.as_ref().map(|a| a.tries).unwrap_or(0) >= 3 {
    return Err(AuthError::Unauthorized);
  }

  let parsed_hash = PasswordHash::new(&db_user.password_hash)
    .map_err(|err| AuthError::Internal(err.to_string().into()))?;

  ARGON2
    .verify_password(password.as_bytes(), &parsed_hash)
    .map_err(|err| {
      ATTEMPTS.insert(
        db_user.email.to_string(),
        attempts
          .map(|a| FailedAttempt { tries: a.tries + 1 })
          .unwrap_or_default(),
      );

      return match err {
        argon2::password_hash::Error::Password => AuthError::Unauthorized,
        err => AuthError::Internal(err.to_string().into()),
      };
    })?;

  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_password_verification() {
    let password = "0123456789.";
    let db_user = DbUser::new_for_test("foo@test.org", password);

    assert!(verify_password(&db_user, password, false).is_ok());

    // Lockout after 3 failed attempts.
    assert!(verify_password(&db_user, "", false).is_err());
    assert!(verify_password(&db_user, "mismatch", false).is_err());
    assert!(verify_password(&db_user, "something else", false).is_err());
    assert!(verify_password(&db_user, password, false).is_err());
    assert!(verify_password(&db_user, password, true).is_ok());
  }
}
