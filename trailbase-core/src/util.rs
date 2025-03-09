use axum::http::HeaderMap;
use base64::prelude::*;
use reqwest::header::AsHeaderName;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Error)]
pub enum IdError {
  #[error("Id error: {0}")]
  InvalidLength(usize),
  #[error("Id error: {0}")]
  Decode(#[from] base64::DecodeSliceError),
}

pub fn b64_to_id(b64_id: &str) -> Result<[u8; 16], IdError> {
  let mut buffer: [u8; 16] = [0; 16];
  let len = BASE64_URL_SAFE.decode_slice(b64_id, &mut buffer)?;
  if len != 16 {
    return Err(IdError::InvalidLength(len));
  }
  return Ok(buffer);
}

pub fn id_to_b64(id: &[u8; 16]) -> String {
  return BASE64_URL_SAFE.encode(id);
}

pub fn uuid_to_b64(uuid: &Uuid) -> String {
  return BASE64_URL_SAFE.encode(uuid.into_bytes());
}

pub fn b64_to_uuid(b64_id: &str) -> Result<Uuid, IdError> {
  return Ok(Uuid::from_bytes(b64_to_id(b64_id)?));
}

pub fn urlencode(s: &str) -> String {
  return form_urlencoded::byte_serialize(s.as_bytes()).collect();
}

#[cfg(debug_assertions)]
#[inline(always)]
pub(crate) fn assert_uuidv7(id: &[u8; 16]) {
  assert_uuidv7_version(&Uuid::from_bytes(*id));
}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub(crate) fn assert_uuidv7(_id: &[u8; 16]) {}

#[cfg(debug_assertions)]
pub(crate) fn assert_uuidv7_version(uuid: &Uuid) {
  let version = uuid.get_version_num();
  if version != 7 {
    panic!("Expected UUIDv7, got UUIDv{version} from: {uuid}");
  }
}

#[cfg(not(debug_assertions))]
pub(crate) fn assert_uuidv7_version(_uuid: &Uuid) {}

pub async fn query_one_row(
  conn: &trailbase_sqlite::Connection,
  sql: &str,
  params: impl trailbase_sqlite::Params + Send + 'static,
) -> Result<trailbase_sqlite::Row, trailbase_sqlite::Error> {
  if let Some(row) = conn.query_row(sql, params).await? {
    return Ok(row);
  }
  return Err(trailbase_sqlite::Error::Rusqlite(
    rusqlite::Error::QueryReturnedNoRows,
  ));
}

#[inline]
pub(crate) fn get_header(headers: &HeaderMap, header_name: impl AsHeaderName) -> Option<&str> {
  if let Some(header) = headers.get(header_name) {
    return header.to_str().ok();
  }
  return None;
}

#[inline]
pub(crate) fn get_header_owned(
  headers: &HeaderMap,
  header_name: impl AsHeaderName,
) -> Option<String> {
  if let Some(header) = headers.get(header_name) {
    if let Ok(str) = header.to_str() {
      return Some(str.to_string());
    }
  }
  return None;
}
