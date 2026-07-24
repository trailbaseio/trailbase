use const_format::formatcp;
use thiserror::Error;

use crate::Name;
use crate::db::{Error as DbError, Transaction, TxError, Value, query};

#[derive(Error, Debug)]
pub enum PrefsError {
  #[error("NotFound")]
  NotFound,
  #[error("Serialization")]
  Serialization,
  #[error("DB: {0}")]
  DB(#[from] DbError),
  #[error("Tx: {0}")]
  Tx(#[from] TxError),
}

type KeyValueStore = std::collections::btree_map::BTreeMap<String, String>;

// TODO: When WASIp3 actually works ,we should probably have dedicated [set|get]_prefs endpoints
// and push the component name mapping responsibility into the host. Would also allow for cross
// request caching and invalidation.
pub async fn get_prefs(component_name: &Name, key: &str) -> Result<Option<String>, PrefsError> {
  let cells = query(QUERY, vec![Value::Text(component_name.name.to_string())]).await?;

  if let Some(Value::Text(json)) = cells.first().and_then(|r| r.first()) {
    let mut store: KeyValueStore =
      serde_json::from_str(json).map_err(|_err| PrefsError::Serialization)?;

    return Ok(store.remove(key));
  }
  return Ok(None);
}

// TODO: When WASIp3 actually works ,we should probably have dedicated [set|get]_prefs endpoints
// and push the component name mapping responsibility into the host. Would also allow for cross
// request caching and invalidation.
pub async fn set_prefs(
  name: &Name,
  key: &str,
  value: impl std::string::ToString,
) -> Result<(), PrefsError> {
  let mut tx = Transaction::begin()?;

  let params = vec![Value::Text(name.name.to_string())];
  let cells = tx.query(QUERY, &params)?;

  let mut map: std::collections::BTreeMap<String, String> =
    if let Some(Value::Text(json)) = cells.first().and_then(|r| r.first()) {
      serde_json::from_str(json).map_err(|_err| PrefsError::Serialization)?
    } else {
      Default::default()
    };

  let _ = map.insert(key.to_string(), value.to_string());

  let params = vec![
    Value::Text(name.name.to_string()),
    Value::Text(serde_json::to_string(&map).map_err(|_err| PrefsError::Serialization)?),
  ];

  tx.execute(
    formatcp!(
      "INSERT INTO {TABLE_NAME} (component, value) VALUES (?1, ?2) \
        ON CONFLICT (component) DO UPDATE SET value= EXCLUDED.value"
    ),
    &params,
  )?;

  tx.commit()?;

  return Ok(());
}

const QUERY: &str = formatcp!("SELECT value FROM {TABLE_NAME} WHERE component = ?1");
const TABLE_NAME: &str = "_wasm_shared_preferences";
