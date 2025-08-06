use argon2::{
  Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
  password_hash::{SaltString, rand_core::OsRng},
};
use rusqlite::Error;
use rusqlite::functions::Context;
use std::sync::LazyLock;

static ARGON2: LazyLock<Argon2<'static>> = LazyLock::new(Argon2::default);

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
  let salt = SaltString::generate(&mut OsRng);
  let hash = ARGON2.hash_password(password.as_bytes(), &salt)?;
  return Ok(hash.to_string());
}

pub fn verify_password(
  password: &[u8],
  hash: &PasswordHash<'_>,
) -> Result<(), argon2::password_hash::Error> {
  return ARGON2.verify_password(password, hash);
}

pub(super) fn hash_password_sqlite(context: &Context) -> Result<String, Error> {
  #[cfg(debug_assertions)]
  if context.len() != 1 {
    return Err(Error::InvalidParameterCount(context.len(), 1));
  }

  let hash = hash_password(context.get_raw(0).as_str()?)
    .map_err(|err| Error::UserFunctionError(format!("Argon2: {err}").into()))?;

  return Ok(hash);
}
