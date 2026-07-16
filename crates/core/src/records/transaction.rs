use axum::extract::{Json, State};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::QualifiedName;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::config::proto::ConflictResolutionStrategy;
use crate::records::params::LazyParams;
use crate::records::record_api::RecordApi;
use crate::records::write_queries::WriteQuery;
use crate::records::{Permission, RecordError};
use crate::util::uuid_to_b64;

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
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

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, TS)]
pub struct TransactionRequest {
  operations: Vec<Operation>,
  transaction: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, TS)]
pub enum TransactionResult {
  Id(String),
  Error(String),
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema, TS)]
#[ts(export)]
pub struct TransactionResponse {
  /// A 1:1 mapping of opreations to results.
  pub results: Vec<TransactionResult>,
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
  // NOTE: We may want to make this user-configurable. The cost also heavily depends on whether
  // `request.transaction == true`.
  match request.operations.len() {
    0 => {
      return Ok(Json(TransactionResponse { results: vec![] }));
    }
    n if n > 128 => {
      return Err(RecordError::BadRequest("Batch size exceeds limit: 128"));
    }
    _ => {}
  }

  let Some(first_api) = request.operations.first().and_then(|op| {
    let api_name = match op {
      Operation::Create { api_name, .. } => api_name,
      Operation::Update { api_name, .. } => api_name,
      Operation::Delete { api_name, .. } => api_name,
    };

    return get_api(&state, api_name).ok();
  }) else {
    return Err(RecordError::BadRequest("empty ops?"));
  };

  let conn = first_api.conn().clone();
  let results = if request.transaction.unwrap_or(false) {
    conn
      .transaction({
        move |mut tx| -> Result<_, trailbase_sqlite::Error> {
          let results = apply_ops(
            &state,
            &mut tx,
            user.as_ref(),
            &first_api,
            request.operations,
          )
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          tx.commit()?;

          return Ok(results);
        }
      })
      .await?
  } else {
    conn
      .call_writer(move |mut conn| -> Result<_, trailbase_sqlite::Error> {
        let results = apply_ops(
          &state,
          &mut conn,
          user.as_ref(),
          &first_api,
          request.operations,
        )
        .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

        return Ok(results);
      })
      .await?
  };

  return Ok(Json(TransactionResponse { results }));
}

#[inline]
fn extract_record_id(value: trailbase_sqlite::Value) -> Result<String, trailbase_sqlite::Error> {
  return match value {
    trailbase_sqlite::Value::Blob(blob) => Ok(BASE64_URL_SAFE.encode(blob)),
    trailbase_sqlite::Value::Text(text) => Ok(text),
    trailbase_sqlite::Value::Integer(i) => Ok(i.to_string()),
    _ => Err(trailbase_sqlite::Error::Other(
      "Unexpected data type".into(),
    )),
  };
}

#[inline]
fn get_db_name(name: &QualifiedName) -> &str {
  return name.database_schema.as_deref().unwrap_or("main");
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

fn get_api(state: &AppState, api_name: &str) -> Result<RecordApi, RecordError> {
  let api = state
    .lookup_record_api(api_name)
    .ok_or_else(|| RecordError::ApiNotFound)?;

  if !api.is_table() {
    return Err(RecordError::ApiRequiresTable);
  }

  return Ok(api);
}

/// Applies operations and returns a list of ids.
fn apply_ops<T: SyncConnection>(
  state: &AppState,
  conn: &mut T,
  user: Option<&User>,
  api: &RecordApi,
  ops: Vec<Operation>,
) -> Result<Vec<TransactionResult>, RecordError> {
  let expected_db_name = get_db_name(api.qualified_name());

  let results: Vec<TransactionResult> = ops
    .into_iter()
    .map(|op| -> Result<TransactionResult, RecordError> {
      return match op {
        Operation::Create { api_name, value } => {
          let api = get_api(state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let mut record = extract_record(value)?;

          if api.insert_autofill_missing_user_id_columns()
            && let Some(user) = user
          {
            for column_index in api.user_id_columns() {
              let col_name = &api.columns()[*column_index].column.name;
              if !record.contains_key(col_name) {
                record.insert(
                  col_name.to_owned(),
                  serde_json::Value::String(uuid_to_b64(&user.uuid)),
                );
              }
            }
          }

          let mut lazy_params =
            LazyParams::for_insert(&api, state.json_schema_registry().clone(), record, None);
          api.record_level_access_check(
            conn,
            Permission::Create,
            None,
            Some(&mut lazy_params),
            user,
          )?;

          let conflict_resolution_strategy = api
            .insert_conflict_resolution_strategy()
            .unwrap_or(ConflictResolutionStrategy::Undefined);

          let (query, _files) = WriteQuery::new_insert_or_replace(
            conn.connection_type(),
            api.table_name(),
            api.columns(),
            &api.record_pk_column().column.name,
            conflict_resolution_strategy,
            lazy_params
              .consume()
              .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          match query.apply_sync(conn) {
            Ok(result) => Ok(TransactionResult::Id(extract_record_id(
              result
                .pk_value
                .ok_or_else(|| RecordError::Internal("insert missing id".into()))?,
            )?)),
            // Skip over errors for `Ignore` conflict strategy.
            Err(err)
              if conflict_resolution_strategy == ConflictResolutionStrategy::Ignore
                && matches!(err, trailbase_sqlite::Error::QueryReturnedNoRows) =>
            {
              Ok(TransactionResult::Error(err.to_string()))
            }
            Err(err) => Err(RecordError::Internal(err.into())),
          }
        }
        Operation::Update {
          api_name,
          record_id: record_id_str,
          value,
        } => {
          let api = get_api(state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let record = extract_record(value)?;
          let record_id = api.primary_key_to_value(record_id_str.clone())?;
          let pk_meta = api.record_pk_column();

          let mut lazy_params = LazyParams::for_update(
            &api,
            state.json_schema_registry().clone(),
            record,
            None,
            pk_meta.column.name.clone(),
            record_id.clone(),
          );

          api.record_level_access_check(
            conn,
            Permission::Update,
            Some(&record_id),
            Some(&mut lazy_params),
            user,
          )?;

          let (query, _files) = WriteQuery::new_update(
            conn.connection_type(),
            api.table_name(),
            lazy_params
              .consume()
              .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let _ = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(TransactionResult::Id(record_id_str))
        }
        Operation::Delete {
          api_name,
          record_id: record_id_str,
        } => {
          let api = get_api(state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let record_id = api.primary_key_to_value(record_id_str.clone())?;

          api.record_level_access_check(conn, Permission::Delete, Some(&record_id), None, user)?;

          let query = WriteQuery::new_delete(
            conn.connection_type(),
            api.table_name(),
            &api.record_pk_column().column.name,
            record_id.clone(),
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let _ = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(TransactionResult::Id(record_id_str))
        }
      };
    })
    .collect::<Result<Vec<_>, _>>()?;

  return Ok(results);
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;
  use crate::app_state::*;
  use crate::config::proto::{PermissionFlag, RecordApiConfig};
  use crate::records::test_utils::*;

  #[tokio::test]
  async fn test_transactions() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE test (
            id      {serial} PRIMARY KEY,
            value   INTEGER
          ) {strict};
        "#,
        strict = strict(conn),
        serial = serial_column(conn),
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    let get_value = async move |id: i64| {
      return conn
        .read_query_row_get::<i64>("SELECT value FROM test WHERE id = $1;", (id,), 0)
        .await
        .unwrap()
        .unwrap();
    };

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
            value: json!({"id": 5, "value": -1}),
          },
        ],
        transaction: None,
      }),
    )
    .await
    .unwrap();

    assert_eq!(2, response.results.len());
    let TransactionResult::Id(first_id) = response.results[0].clone() else {
      panic!("Not an id: {response:?}");
    };
    let second_id = {
      let TransactionResult::Id(id) = response.results[1].clone() else {
        panic!("Not an id: {response:?}");
      };
      id.parse::<i64>().unwrap()
    };
    assert_eq!(5, second_id, "{:?}", response.results);
    assert_eq!(-1, get_value(second_id).await,);

    // Make sure replace works.
    let response = record_transactions_handler(
      State(state.clone()),
      None,
      Json(TransactionRequest {
        operations: vec![Operation::Create {
          api_name: "test_api".to_string(),
          value: json!({"id": 5, "value": 2}),
        }],
        transaction: None,
      }),
    )
    .await
    .unwrap();

    assert_eq!(1, response.results.len());
    let id = {
      let TransactionResult::Id(id) = response.results[0].clone() else {
        panic!("Not an id: {response:?}");
      };
      id.parse::<i64>().unwrap()
    };
    assert_eq!(5, id, "{:?}", response.results);
    assert_eq!(2, get_value(id).await);

    let response = record_transactions_handler(
      State(state.clone()),
      None,
      Json(TransactionRequest {
        operations: vec![
          Operation::Delete {
            api_name: "test_api".to_string(),
            record_id: first_id,
          },
          Operation::Create {
            api_name: "test_api".to_string(),
            value: json!({"value": 3}),
          },
        ],
        transaction: None,
      }),
    )
    .await
    .unwrap();

    assert_eq!(2, response.results.len());

    assert_eq!(
      2,
      conn
        .read_query_value::<i64>("SELECT COUNT(*) FROM test;", ())
        .await
        .unwrap()
        .unwrap()
    );

    // Test ignore strategy
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("test_api_ignore".to_string()),
        table_name: Some("test".to_string()),
        conflict_resolution: Some(ConflictResolutionStrategy::Ignore as i32),
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

    // Make sure ignore works.
    let response = record_transactions_handler(
      State(state.clone()),
      None,
      Json(TransactionRequest {
        operations: vec![Operation::Create {
          api_name: "test_api_ignore".to_string(),
          value: json!({"id": 5, "value": -5}),
        }],
        transaction: None,
      }),
    )
    .await
    .unwrap();

    assert_eq!(1, response.results.len());
  }
}
