use argon2::{
  password_hash::{rand_core::OsRng, SaltString},
  Argon2, PasswordHasher,
};

use crate::auth::AuthError;

pub struct PasswordOptions {
  pub min_length: usize,
  pub max_length: usize,
}

impl PasswordOptions {
  pub const fn default() -> Self {
    PasswordOptions {
      min_length: 8,
      max_length: 128,
    }
  }
}

pub fn validate_passwords(
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

pub fn hash_password(password: &str) -> Result<String, AuthError> {
  let salt = SaltString::generate(&mut OsRng);
  return Ok(
    Argon2::default()
      .hash_password(password.as_bytes(), &salt)
      .map_err(|err| {
        // NOTE: Wrapping needed since Argon's error doesn't implement the error trait.
        AuthError::Internal(err.to_string().into())
      })?
      .to_string(),
  );
}
