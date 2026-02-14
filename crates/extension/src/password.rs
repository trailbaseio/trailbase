use argon2::password_hash::{SaltString, rand_core::OsRng};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use rusqlite::Error;
use rusqlite::functions::{Context, FunctionFlags};
use std::str::FromStr;
use std::sync::LazyLock;

#[derive(Debug, thiserror::Error, PartialEq)]
#[non_exhaustive]
pub enum PasswordError {
  #[error("InvalidPassword")]
  InvalidPassword,

  #[error("InvalidHash")]
  InvalidHash,

  #[error("OtherError")]
  Other,
}

impl From<argon2::password_hash::Error> for PasswordError {
  fn from(value: argon2::password_hash::Error) -> Self {
    return match value {
      argon2::password_hash::Error::Password => Self::InvalidPassword,
      argon2::password_hash::Error::PhcStringTrailingData => Self::InvalidHash,
      argon2::password_hash::Error::PhcStringField => Self::InvalidHash,
      _err => Self::Other,
    };
  }
}

impl From<bcrypt::BcryptError> for PasswordError {
  fn from(value: bcrypt::BcryptError) -> Self {
    return match value {
      bcrypt::BcryptError::InvalidHash(_) => Self::InvalidHash,
      _err => Self::Other,
    };
  }
}

static ARGON2: LazyLock<Argon2<'static>> = LazyLock::new(Argon2::default);

pub fn hash_password(password: &str) -> Result<String, PasswordError> {
  let salt = SaltString::generate(&mut OsRng);
  let hash = ARGON2.hash_password(password.as_bytes(), &salt)?;
  return Ok(hash.to_string());
}

pub fn verify_password<P: AsRef<[u8]>>(password: P, hash: &str) -> Result<(), PasswordError> {
  if let Ok(parsed_hash) = PasswordHash::new(hash) {
    return Ok(ARGON2.verify_password(password.as_ref(), &parsed_hash)?);
  }

  // We have a fallback to bcrypt to support hashes imported from external providers, e.g. Auth0.
  if let Ok(_parsed_hash) = bcrypt::HashParts::from_str(hash) {
    return match bcrypt::verify(password, hash)? {
      true => Ok(()),
      false => Err(PasswordError::InvalidPassword),
    };
  }

  // Hash was neither a valid argon2 nor a bcrypt hash. This can only happen if an invalid hash was
  // inserted manually.
  return Err(PasswordError::InvalidHash);
}

pub fn valid_hash(hash: &str) -> bool {
  if PasswordHash::new(hash).is_ok() {
    return true;
  }
  if bcrypt::HashParts::from_str(hash).is_ok() {
    return true;
  }
  return false;
}

/// An SQLite extension function we can use, e.g. to prefill users from migrations.
fn hash_password_sqlite(context: &Context) -> Result<String, Error> {
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let hash = hash_password(context.get_raw(0).as_str()?)
    .map_err(|err| Error::UserFunctionError(format!("Argon2: {err}").into()))?;

  return Ok(hash);
}

pub(crate) fn register_extension_functions(db: &rusqlite::Connection) -> Result<(), Error> {
  // WARN: Be careful with declaring INNOCUOUS. It allows "user-defined functions" to run
  // when "trusted_schema=OFF", which means as part of: VIEWs, TRIGGERs, CHECK, DEFAULT,
  // GENERATED cols, ... as opposed to just top-level SELECTs.

  // Used to create initial user credentials in migrations.
  db.create_scalar_function(
    "hash_password",
    1,
    FunctionFlags::SQLITE_UTF8
      | FunctionFlags::SQLITE_DETERMINISTIC
      | FunctionFlags::SQLITE_INNOCUOUS,
    hash_password_sqlite,
  )?;

  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;
  #[test]
  fn test_verification() {
    let argon2_hash = "$argon2id$v=19$m=19456,t=2,p=1$QU8j9dRSzZ2tn/1e3BwgMg$yfperWEmAfO/7UJipZ3C7OEl4dYkjvRfr2CH4UgdE5E";
    assert_eq!(Ok(()), verify_password("secret", argon2_hash));
    assert_eq!(
      Err(PasswordError::InvalidPassword),
      verify_password("wrong", argon2_hash)
    );
    let argon2_invalid_hash = "$argon2id$v=19$m=19456,t=2,p=1$QU8j9$RSzZ2tn/1e3BwgMg$yfperWEmAfO/7UJipZ3C7OEl4dYkjvRfr2CH4UgdE5E";
    assert_eq!(
      Err(PasswordError::InvalidHash),
      verify_password("secret", argon2_invalid_hash)
    );

    let bcrypt_hash = "$2b$12$OziW5BRZpnl8FDOkOzqcxe/SFfq3n0sClAQHA6UnfT2Hl.mvtDDOi";
    assert_eq!(Ok(()), verify_password("secret", bcrypt_hash));
    assert_eq!(
      Err(PasswordError::InvalidPassword),
      verify_password("wrong", bcrypt_hash)
    );

    assert_eq!(
      Err(PasswordError::InvalidHash),
      verify_password("secret", "invalid_hash")
    );
  }
}
