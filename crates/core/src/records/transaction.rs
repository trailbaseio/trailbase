use axum::extract::{Json, State};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use trailbase_schema::QualifiedName;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::config::proto::ConflictResolutionStrategy;
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
  transaction: Option<bool>,
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
  // NOTE: We may want to make this user-configurable. The cost also heavily depends on whether
  // `request.transaction == true`.
  match request.operations.len() {
    0 => {
      return Ok(Json(TransactionResponse { ids: vec![] }));
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
  let ids = if request.transaction.unwrap_or(false) {
    conn
      .transaction({
        move |mut tx| -> Result<Vec<String>, trailbase_sqlite::Error> {
          let ids: Vec<String> = apply_ops(
            &state,
            &mut tx,
            user.as_ref(),
            &first_api,
            request.operations,
          )
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          tx.commit()?;

          return Ok(ids);
        }
      })
      .await?
  } else {
    conn
      .call_writer(
        move |mut conn| -> Result<Vec<String>, trailbase_sqlite::Error> {
          let ids: Vec<String> = apply_ops(
            &state,
            &mut conn,
            user.as_ref(),
            &first_api,
            request.operations,
          )
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          return Ok(ids);
        },
      )
      .await?
  };

  return Ok(Json(TransactionResponse { ids }));
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

fn apply_ops<T: SyncConnection>(
  state: &AppState,
  conn: &mut T,
  user: Option<&User>,
  api: &RecordApi,
  ops: Vec<Operation>,
) -> Result<Vec<String>, RecordError> {
  let expected_db_name = get_db_name(api.qualified_name());

  let ids: Vec<String> = ops
    .into_iter()
    .map(|op| -> Result<Option<String>, RecordError> {
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

          #[inline]
          fn is_no_rows_err(err: &trailbase_sqlite::Error) -> bool {
            // NOTE: We're turning non-result into rusqlite errors even for postgres.
            // We should change that.
            return match err {
              trailbase_sqlite::Error::Rusqlite(rusqlite::Error::QueryReturnedNoRows) => true,
              _ => false,
            };
          }

          match query.apply_sync(conn) {
            Ok(result) => Ok(Some(
              extract_record_id(result.pk_value.expect("insert"))
                .map_err(|err| RecordError::Internal(err.into()))?,
            )),
            Err(err)
              if conflict_resolution_strategy == ConflictResolutionStrategy::Ignore
                && is_no_rows_err(&err) =>
            {
              Ok(None)
            }
            Err(err) => Err(RecordError::Internal(err.into())),
          }
        }
        Operation::Update {
          api_name,
          record_id,
          value,
        } => {
          let api = get_api(state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let record = extract_record(value)?;
          let record_id = api.primary_key_to_value(record_id)?;
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

          Ok(None)
        }
        Operation::Delete {
          api_name,
          record_id,
        } => {
          let api = get_api(state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let record_id = api.primary_key_to_value(record_id)?;

          api.record_level_access_check(conn, Permission::Delete, Some(&record_id), None, user)?;

          let query = WriteQuery::new_delete(
            conn.connection_type(),
            api.table_name(),
            &api.record_pk_column().column.name,
            record_id,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let _ = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(None)
        }
      };
    })
    .collect::<Result<Vec<_>, _>>()?
    .into_iter()
    .flatten()
    .collect();

  return Ok(ids);
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
        strict = strict2(conn),
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

    assert_eq!(2, response.ids.len());
    let first_id = response.ids[0].clone();
    let second_id = response.ids[1].parse::<i64>().unwrap();
    assert_eq!(5, second_id, "{:?}", response.ids);
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

    assert_eq!(1, response.ids.len());
    let id = response.ids[0].parse::<i64>().unwrap();
    assert_eq!(5, id, "{:?}", response.ids);
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

    assert_eq!(1, response.ids.len());

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

    assert_eq!(0, response.ids.len());
  }
}
