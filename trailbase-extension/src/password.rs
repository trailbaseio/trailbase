use argon2::{password_hash::SaltString, Argon2, PasswordHasher};
use rand::rngs::OsRng;
use sqlite_loadable::prelude::*;
use sqlite_loadable::{api, Error, ErrorKind};

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
  let salt = SaltString::generate(&mut OsRng);
  let hash = Argon2::default().hash_password(password.as_bytes(), &salt)?;
  return Ok(hash.to_string());
}

pub(super) fn hash_password_sqlite(
  context: *mut sqlite3_context,
  values: &[*mut sqlite3_value],
) -> Result<(), Error> {
  if values.len() != 1 {
    return Err(Error::new_message("Expected 1 argument"));
  }

  let value = &values[0];
  match api::value_type(value) {
    api::ValueType::Text => {
      let contents = api::value_text(value)?;
      let hash = hash_password(contents)
        .map_err(|err| Error::new(ErrorKind::Message(format!("Argon2: {err}"))))?;

      api::result_text(context, hash)?;
    }
    _ => {
      return Err(Error::new_message("Expected 1 argument of type TEXT"));
    }
  };

  return Ok(());
}
