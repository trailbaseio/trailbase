use bytes::Bytes;
use const_format::formatcp;
use http_body_util::{BodyExt, combinators::UnsyncBoxBody};
use trailbase_sqlite::{SyncConnectionTrait, params, traits::SyncTransaction};
use trailbase_wasm_common::{PrefsRequest, PrefsResponse};
use wasmtime_wasi_http::p2::bindings::http::types::ErrorCode;

type KeyValueStore = std::collections::btree_map::BTreeMap<String, String>;

pub(crate) async fn handle_prefs_request(
  conn: trailbase_sqlite::Connection,
  component_name: String,
  request: http::Request<wasmtime_wasi_http::WasiBody>,
) -> Result<http::Response<wasmtime_wasi_http::WasiBody>, wasmtime_wasi_http::Error> {
  let prefs_request = match to_request(request).await {
    Ok(request) => request,
    Err(err) => {
      return to_response(PrefsResponse::Error(err));
    }
  };

  return match prefs_request {
    PrefsRequest::Get { key } => {
      async fn get_pref(
        conn: trailbase_sqlite::Connection,
        component_name: String,
        key: String,
      ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let json: Option<String> = conn
          .write_query_row_get(SELECT_QUERY, params!(component_name), 0)
          .await?;

        if let Some(json) = json {
          let mut store: KeyValueStore = serde_json::from_str(&json)?;
          return Ok(store.remove(&key));
        }
        return Ok(None);
      }

      match get_pref(conn, component_name, key).await {
        Ok(value) => to_response(PrefsResponse::Value(value)),
        Err(err) => to_response(PrefsResponse::Error(err.to_string())),
      }
    }
    PrefsRequest::Set { key, value } => {
      async fn set_pref(
        conn: trailbase_sqlite::Connection,
        component_name: String,
        key: String,
        value: Option<String>,
      ) -> Result<(), Box<dyn std::error::Error>> {
        conn
          .transaction(move |mut tx| -> Result<(), trailbase_sqlite::Error> {
            let row = tx.query_row(SELECT_QUERY, params!(component_name.clone()))?;

            let mut map: KeyValueStore = match row {
              Some(row) if let Some(json) = row.get::<Option<String>>(0)? => {
                serde_json::from_str(&json)
                  .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?
              }
              _ => Default::default(),
            };

            if let Some(value) = value {
              let _ = map.insert(key.to_string(), value);
            } else {
              let _ = map.remove(&key);
            }

            tx.execute(
              formatcp!(
                "INSERT INTO {TABLE_NAME} (component, value) VALUES (?1, ?2) \
                   ON CONFLICT (component) DO UPDATE SET value= EXCLUDED.value"
              ),
              params!(
                component_name,
                serde_json::to_string(&map)
                  .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?,
              ),
            )?;

            return tx.commit();
          })
          .await?;

        return Ok(());
      }

      match set_pref(conn, component_name, key, value).await {
        Ok(_) => to_response(PrefsResponse::Ok),
        Err(err) => to_response(PrefsResponse::Error(err.to_string())),
      }
    }
  };
}

async fn to_request(
  request: http::Request<wasmtime_wasi_http::WasiBody>,
) -> Result<PrefsRequest, String> {
  let (_parts, body) = request.into_parts();
  let bytes: Bytes = body
    .collect()
    .await
    .map_err(|err| err.to_string())?
    .to_bytes();
  return serde_json::from_slice(&bytes).map_err(|err| err.to_string());
}

fn to_response(
  response: PrefsResponse,
) -> Result<http::Response<wasmtime_wasi_http::WasiBody>, wasmtime_wasi_http::Error> {
  let body =
    serde_json::to_vec(&response).map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  let resp = http::Response::builder()
    .status(200)
    .body(bytes_to_body(Bytes::from_owner(body)))
    .map_err(|err| ErrorCode::InternalError(Some(err.to_string())))?;

  return Ok(resp);
}

#[inline]
pub fn bytes_to_body<E>(bytes: Bytes) -> UnsyncBoxBody<Bytes, E> {
  UnsyncBoxBody::new(http_body_util::Full::new(bytes).map_err(|_| unreachable!()))
}

const TABLE_NAME: &str = "_wasm_shared_preferences";
const SELECT_QUERY: &str = formatcp!("SELECT value FROM {TABLE_NAME} WHERE component = ?1");
