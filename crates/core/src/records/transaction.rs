use axum::extract::{Json, State};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::records::params::LazyParams;
use crate::records::record_api::RecordApi;
use crate::records::write_queries::WriteQuery;
use crate::records::{Permission, RecordError};
use crate::util::uuid_to_b64;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub enum Operation {
  Create {
    api_name: String,
    value: serde_json::Value,
  },
  Update {
    api_name: String,
    record_id: String,
    value: serde_json::Value,
  },
  Delete {
    api_name: String,
    record_id: String,
  },
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct TransactionRequest {
  operations: Vec<Operation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct TransactionResponse {
  /// Url-Safe base64 encoded ids of the newly created record.
  pub ids: Vec<String>,
}

/// Execute a batch of transactions.
#[utoipa::path(
  post,
  path = "/api/transaction/v1/execute",
  tag = "transactions",
  params(),
  request_body = TransactionRequest,
  responses(
    (status = 200, description = "Ids of successfully created records.", body = TransactionResponse),
  )
)]
pub async fn record_transactions_handler(
  State(state): State<AppState>,
  user: Option<User>,
  Json(request): Json<TransactionRequest>,
) -> Result<Json<TransactionResponse>, RecordError> {
  if request.operations.len() > 128 {
    return Err(RecordError::BadRequest("Transactions exceed limit: 128"));
  }

  type Op = dyn (FnOnce(&rusqlite::Connection) -> Result<Option<String>, RecordError>) + Send;

  let operations: Vec<Box<Op>> = request
    .operations
    .into_iter()
    .map(|op| -> Result<Box<Op>, RecordError> {
      return match op {
        Operation::Create { api_name, value } => {
          let api = get_api(&state, &api_name)?;
          let mut record = extract_record(value)?;

          if api.insert_autofill_missing_user_id_columns() {
            if let Some(ref user) = user {
              for column_index in api.user_id_columns() {
                let col_name = &api.columns()[*column_index].name;
                if !record.contains_key(col_name) {
                  record.insert(
                    col_name.to_owned(),
                    serde_json::Value::String(uuid_to_b64(&user.uuid)),
                  );
                }
              }
            }
          }

          let mut lazy_params = LazyParams::for_insert(&api, record, None);
          let acl_check = api.build_record_level_access_check(
            Permission::Create,
            None,
            Some(&mut lazy_params),
            user.as_ref(),
          )?;

          let (query, _files) = WriteQuery::new_insert(
            api.table_name(),
            &api.record_pk_column().1.name,
            api.insert_conflict_resolution_strategy(),
            lazy_params
              .consume()
              .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Box::new(move |conn| {
            acl_check(conn)?;
            let result = query
              .apply(conn)
              .map_err(|err| RecordError::Internal(err.into()))?;

            return Ok(Some(
              extract_record_id(result.pk_value.expect("insert"))
                .map_err(|err| RecordError::Internal(err.into()))?,
            ));
          }))
        }
        Operation::Update {
          api_name,
          record_id,
          value,
        } => {
          let api = get_api(&state, &api_name)?;
          let record = extract_record(value)?;
          let record_id = api.primary_key_to_value(record_id)?;
          let (_index, pk_column) = api.record_pk_column();

          let mut lazy_params = LazyParams::for_update(&api, record, None, pk_column.name.clone(), record_id.clone());

          let acl_check = api.build_record_level_access_check(
            Permission::Update,
            Some(&record_id),
            Some(&mut lazy_params),
            user.as_ref(),
          )?;

          let (query, _files) = WriteQuery::new_update(
            api.table_name(),
            lazy_params
              .consume()
              .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Box::new(move |conn| {
            acl_check(conn)?;
            let _ = query
              .apply(conn)
              .map_err(|err| RecordError::Internal(err.into()))?;

            return Ok(None);
          }))
        }
        Operation::Delete {
          api_name,
          record_id,
        } => {
          let api = get_api(&state, &api_name)?;
          let record_id = api.primary_key_to_value(record_id)?;

          let acl_check = api.build_record_level_access_check(
            Permission::Delete,
            Some(&record_id),
            None,
            user.as_ref(),
          )?;

          let query =
            WriteQuery::new_delete(api.table_name(), &api.record_pk_column().1.name, record_id)
              .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Box::new(move |conn| {
            acl_check(conn)?;
            let _ = query
              .apply(conn)
              .map_err(|err| RecordError::Internal(err.into()))?;

            return Ok(None);
          }))
        }
      };
    })
    .collect::<Result<Vec<_>, _>>()?;

  let ids = state
    .conn()
    .call(
      move |conn: &mut rusqlite::Connection| -> Result<Vec<String>, trailbase_sqlite::Error> {
        let tx = conn.transaction()?;

        let mut ids: Vec<String> = vec![];
        for op in operations {
          if let Some(id) = op(&tx).map_err(|err| trailbase_sqlite::Error::Other(err.into()))? {
            ids.push(id);
          }
        }

        tx.commit()?;

        return Ok(ids);
      },
    )
    .await?;

  return Ok(Json(TransactionResponse { ids }));
}

#[inline]
fn extract_record_id(value: rusqlite::types::Value) -> Result<String, trailbase_sqlite::Error> {
  return match value {
    rusqlite::types::Value::Blob(blob) => Ok(BASE64_URL_SAFE.encode(blob)),
    rusqlite::types::Value::Text(text) => Ok(text),
    rusqlite::types::Value::Integer(i) => Ok(i.to_string()),
    _ => Err(trailbase_sqlite::Error::Other(
      "Unexpected data type".into(),
    )),
  };
}

#[inline]
fn get_api(state: &AppState, api_name: &str) -> Result<RecordApi, RecordError> {
  let Some(api) = state.lookup_record_api(api_name) else {
    return Err(RecordError::ApiNotFound);
  };
  if !api.is_table() {
    return Err(RecordError::ApiRequiresTable);
  }
  return Ok(api);
}

#[inline]
fn extract_record(
  value: serde_json::Value,
) -> Result<serde_json::Map<String, serde_json::Value>, RecordError> {
  let serde_json::Value::Object(record) = value else {
    return Err(RecordError::BadRequest("Not a record"));
  };
  return Ok(record);
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;
  use crate::app_state::*;
  use crate::config::proto::{ConflictResolutionStrategy, PermissionFlag, RecordApiConfig};
  use crate::records::test_utils::*;

  #[tokio::test]
  async fn test_transactions() {
    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE test (
            id      INTEGER PRIMARY KEY,
            value   INTEGER
          ) STRICT;
        "#,
      )
      .await
      .unwrap();

    state.rebuild_schema_cache().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("test_api".to_string()),
        table_name: Some("test".to_string()),
        conflict_resolution: Some(ConflictResolutionStrategy::Replace as i32),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Create as i32,
          PermissionFlag::Delete as i32,
          PermissionFlag::Read as i32,
        ]
        .into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let response = record_transactions_handler(
      State(state.clone()),
      None,
      Json(TransactionRequest {
        operations: vec![
          Operation::Create {
            api_name: "test_api".to_string(),
            value: json!({"value": 1}),
          },
          Operation::Create {
            api_name: "test_api".to_string(),
            value: json!({"value": 2}),
          },
        ],
      }),
    )
    .await
    .unwrap();

    assert_eq!(2, response.ids.len());

    let response = record_transactions_handler(
      State(state.clone()),
      None,
      Json(TransactionRequest {
        operations: vec![
          Operation::Delete {
            api_name: "test_api".to_string(),
            record_id: response.ids[0].clone(),
          },
          Operation::Create {
            api_name: "test_api".to_string(),
            value: json!({"value": 3}),
          },
        ],
      }),
    )
    .await
    .unwrap();

    assert_eq!(1, response.ids.len());

    assert_eq!(
      2,
      state
        .conn()
        .read_query_value::<i64>("SELECT COUNT(*) FROM test;", ())
        .await
        .unwrap()
        .unwrap()
    );
  }
}
