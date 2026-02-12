use mini_moka::sync::Cache;
use std::sync::LazyLock;

use crate::auth::AuthError;
use crate::auth::user::DbUser;

#[derive(Clone, Debug)]
pub struct PasswordOptions {
  pub min_length: usize,
  pub max_length: usize,

  pub must_contain_upper_and_lower_case: bool,
  pub must_contain_digits: bool,
  pub must_contain_special_characters: bool,
}

impl Default for PasswordOptions {
  fn default() -> Self {
    return PasswordOptions {
      min_length: 8,
      max_length: 128,
      must_contain_upper_and_lower_case: false,
      must_contain_digits: false,
      must_contain_special_characters: false,
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

  if opts.must_contain_digits {
    if !password.chars().any(|x| x.is_numeric()) {
      return Err(AuthError::BadRequest("Must contain digits"));
    }
    if password.chars().all(|x| x.is_numeric()) {
      return Err(AuthError::BadRequest("Must contain non-digits"));
    }
  }

  if opts.must_contain_upper_and_lower_case
    && !(password.chars().any(|x| x.is_lowercase()) && password.chars().any(|x| x.is_uppercase()))
  {
    return Err(AuthError::BadRequest("Must contain lower and upper case"));
  }

  if opts.must_contain_special_characters && password.chars().all(|x| x.is_alphanumeric()) {
    return Err(AuthError::BadRequest("Must contain special characters"));
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

// Track login attempts for abuse prevention.
static ATTEMPTS: LazyLock<Cache<String, FailedAttempt>> = LazyLock::new(|| {
  Cache::builder()
    .time_to_live(std::time::Duration::from_secs(60))
    .max_capacity(1024)
    .build()
});

pub fn hash_password(password: &str) -> Result<String, AuthError> {
  return trailbase_extension::password::hash_password(password).map_err(|err| {
    // NOTE: Wrapping needed since Argon's error doesn't implement the error trait.
    AuthError::Internal(err.to_string().into())
  });
}

/// Checks the given password against a known user. Will further ensure that the email was verified
/// and rate limit attempts to protect against brute-force attacks.
pub fn check_user_password(
  db_user: &DbUser,
  password: &str,
  is_demo: bool,
) -> Result<(), AuthError> {
  if !db_user.verified {
    return Err(AuthError::Unauthorized);
  }
  let attempts = ATTEMPTS.get(&db_user.email);

  if !is_demo && attempts.as_ref().map(|a| a.tries).unwrap_or(0) >= LOGIN_RATE_LIMIT {
    return Err(AuthError::TooManyRequests);
  }

  trailbase_extension::password::verify_password(password.as_bytes(), &db_user.password_hash)
    .map_err(|err| {
      ATTEMPTS.insert(
        db_user.email.to_string(),
        attempts
          .map(|a| FailedAttempt { tries: a.tries + 1 })
          .unwrap_or_default(),
      );

      return match err {
        trailbase_extension::password::PasswordError::InvalidPassword => AuthError::Unauthorized,
        err => AuthError::Internal(err.to_string().into()),
      };
    })?;

  return Ok(());
}

// HACK: Increase limit in tests to avoid limits.
#[cfg(test)]
const LOGIN_RATE_LIMIT: usize = 10;

#[cfg(not(test))]
const LOGIN_RATE_LIMIT: usize = 3;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_password_verification() {
    let password = "0123456789.";
    let db_user = DbUser::new_for_test("foo@test.org", password);

    assert!(check_user_password(&db_user, password, false).is_ok());

    // Lock-out/rate-limit after 10 (3 in prod) failed attempts.
    for _ in 0..10 {
      assert!(check_user_password(&db_user, "mismatch", false).is_err());
    }
    assert!(check_user_password(&db_user, password, false).is_err());

    // By-pass lock-out in demo mode.
    assert!(check_user_password(&db_user, password, true).is_ok());
  }

  #[test]
  fn test_password_policy() {
    let default_options = PasswordOptions::default();
    let password = "abc123ABC";
    assert!(validate_password_policy(password, password, &default_options).is_ok());
    assert!(validate_password_policy(password, "Abc123ABC", &default_options).is_err());

    let test =
      |password: &str, opts: &PasswordOptions| validate_password_policy(password, password, opts);

    {
      // length
      let options = PasswordOptions {
        min_length: 2,
        max_length: 4,
        ..Default::default()
      };

      assert!(test("22", &options).is_ok());
      assert!(test("2222", &options).is_ok());
      assert!(test("2", &options).is_err());
      assert!(test("22222", &options).is_err());
    }

    {
      // lower-upper
      let options = PasswordOptions {
        min_length: 2,
        must_contain_upper_and_lower_case: true,
        ..Default::default()
      };

      assert!(test("22", &options).is_err());
      assert!(test("2a", &options).is_err());
      assert!(test("Aa", &options).is_ok());
    }

    {
      // Must contain digits
      let options = PasswordOptions {
        min_length: 2,
        must_contain_digits: true,
        ..Default::default()
      };

      assert!(test("aa", &options).is_err());
      assert!(test("2a", &options).is_ok());
      assert!(test("22", &options).is_err());
    }

    {
      // Must contain digits
      let options = PasswordOptions {
        min_length: 2,
        must_contain_special_characters: true,
        ..Default::default()
      };

      assert!(test("aa", &options).is_err());
      assert!(test("a2", &options).is_err());
      assert!(test("2.", &options).is_ok());
    }
  }
}
