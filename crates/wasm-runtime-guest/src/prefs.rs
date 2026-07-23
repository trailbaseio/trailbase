use const_format::formatcp;
use thiserror::Error;

use crate::Name;
use crate::db::{Error as DbError, Value, execute, query};

#[derive(Error, Debug)]
pub enum PrefsError {
  #[error("NotFound")]
  NotFound,
  #[error("Serialization")]
  Serialization,
  #[error("DBError: {0}")]
  DB(#[from] DbError),
}

pub async fn get_prefs(name: &Name) -> Result<Option<String>, PrefsError> {
  let cells = query(
    formatcp!("SELECT value FROM {TABLE_NAME} WHERE component = ?1"),
    vec![Value::Text(name.name.to_string())],
  )
  .await?;

  if let Some(Value::Text(text)) = cells.get(0).and_then(|r| r.get(0)) {
    return Ok(Some(text.clone()));
  }
  return Ok(None);
}

pub async fn set_prefs(name: &Name, value: impl std::string::ToString) -> Result<(), PrefsError> {
  execute(
    formatcp!(
      "\
      INSERT INTO {TABLE_NAME} (component, value) VALUES (?1, ?2) \
        ON CONFLICT (component) DO UPDATE SET value= EXCLUDED.value \
    "
    ),
    vec![
      Value::Text(name.name.to_string()),
      Value::Text(value.to_string()),
    ],
  )
  .await?;

  return Ok(());
}

const TABLE_NAME: &str = "_wasm_shared_preferences";
