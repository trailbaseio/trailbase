use std::time::Duration;

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

pub(crate) fn hash_password_impl(password: &str) -> Result<String, AuthError> {
  return trailbase_extension::password::hash_password(password)
    .map_err(|err| AuthError::Internal(err.into()));
}

/// Hashes the given password with argon2.
///
/// NOTE: hashing is a synchronous but expensive op (tens of milliseconds in release mode if you
/// have AVX and hundreds in debug builds), so we push work into a background thread with a fixed
/// timeout to prevent the async runtime from locking up.
pub async fn hash_password(password: String) -> Result<String, AuthError> {
  return tokio::time::timeout(
    HASHING_TIMEOUT,
    tokio::task::spawn_blocking(move || hash_password_impl(&password)),
  )
  .await
  .map_err(|_| AuthError::Timeout)?
  .map_err(|_err| {
    return cfg_select! {
      debug_assertions => AuthError::Internal(_err.into()),
      _ => AuthError::Internal("busy".into()),
    };
  })?;
}

/// Checks the given password against a known user's hash. Will further ensure that an email
/// address, if present, is verified.
///
/// NOTE: hashing is a synchronous but expensive op (tens of milliseconds in release mode if you
/// have AVX and hundreds in debug builds), so we push work into a background thread with a fixed
/// timeout to prevent the async runtime from locking up.
pub async fn check_user_password(db_user: &DbUser, password: String) -> Result<(), AuthError> {
  if db_user.unverified_email.is_some() {
    return Err(AuthError::Unauthorized);
  }

  let Some(password_hash) = db_user.password_hash.clone() else {
    return Err(AuthError::Unauthorized);
  };

  fn check_user_password_impl(password: &str, password_hash: &str) -> Result<(), AuthError> {
    return trailbase_extension::password::verify_password(password, password_hash).map_err(
      |err| {
        return match err {
          trailbase_extension::password::PasswordError::InvalidPassword => AuthError::Unauthorized,
          err => AuthError::Internal(err.to_string().into()),
        };
      },
    );
  }

  return tokio::time::timeout(
    HASHING_TIMEOUT,
    tokio::task::spawn_blocking(move || {
      return check_user_password_impl(&password, &password_hash);
    }),
  )
  .await
  .map_err(|_| AuthError::Timeout)?
  .map_err(|_err| {
    return cfg_select! {
      debug_assertions => AuthError::Internal(_err.into()),
      _ => AuthError::Internal("busy".into()),
    };
  })?;
}

pub(crate) fn measure_password_verification_timing() -> std::time::Duration {
  let hash = hash_password_impl("pw").expect("constant input");
  let started = std::time::Instant::now();
  let _ = trailbase_extension::password::verify_password(b"pw", &hash);
  return started.elapsed();
}

const HASHING_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn test_password_verification() {
    let password = "0123456789.";
    let db_user = DbUser::new_for_test("foo@test.org", password);

    assert!(
      check_user_password(&db_user, password.to_string())
        .await
        .is_ok()
    );
    assert!(
      check_user_password(&db_user, "nonsense".to_string())
        .await
        .is_err()
    );
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
