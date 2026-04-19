use axum::extract::{Json, State};
use base64::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trailbase_schema::QualifiedName;
use trailbase_sqlite::{SyncConnection, SyncConnectionTrait};
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
    // let (conn, operations) = build_deferred_ops::<trailbase_sqlite::Transaction>(
    //   &state,
    //   user.as_ref(),
    //   request.operations,
    // )?;

    conn
      .transaction({
        move |tx| -> Result<Vec<String>, trailbase_sqlite::Error> {
          let ids: Vec<String> =
            apply_ops(&state, &tx, user.as_ref(), &first_api, request.operations)
              .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          tx.commit()?;

          return Ok(ids);
        }
      })
      .await?
  } else {
    conn
      .call_writer2(
        move |conn| -> Result<Vec<String>, trailbase_sqlite::Error> {
          let ids: Vec<String> =
            apply_ops(&state, &conn, user.as_ref(), &first_api, request.operations)
              .map_err(|err| trailbase_sqlite::Error::Other(err.into()))?;

          return Ok(ids);
        },
      )
      .await?

    // let (conn, operations) = build_deferred_ops::<trailbase_sqlite::SyncConnection>(
    //   &state,
    //   user.as_ref(),
    //   request.operations,
    // )?;
    //
    // conn
    //   .call_writer2(
    //     move |conn: SyncConnection| -> Result<Vec<String>, trailbase_sqlite::Error> {
    //       let mut ids: Vec<String> = vec![];
    //       for op in operations {
    //         if let Some(id) = op(&conn).map_err(|err| trailbase_sqlite::Error::Other(err.into()))? {
    //           ids.push(id);
    //         }
    //       }
    //       return Ok(ids);
    //     },
    //   )
    //   .await?
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

// type Op = dyn (FnOnce(&rusqlite::Connection) -> Result<Option<String>, RecordError>) + Send;
// type Op<T: SyncConnectionTrait> = dyn (FnOnce(&T) -> Result<Option<String>, RecordError>) + Send;

// fn build_deferred_ops<T: SyncConnectionTrait + 'static>(
//   state: &AppState,
//   user: Option<&User>,
//   ops: Vec<Operation>,
// ) -> Result<(Arc<trailbase_sqlite::Connection>, Vec<Box<Op<T>>>), RecordError> {
//   let mut db: (String, Option<Arc<trailbase_sqlite::Connection>>) = Default::default();
//   let mut get_api =
//     |state: &AppState, api_name: &str, idx: usize| -> Result<RecordApi, RecordError> {
//       let api = state
//         .lookup_record_api(api_name)
//         .ok_or_else(|| RecordError::ApiNotFound)?;
//       if !api.is_table() {
//         return Err(RecordError::ApiRequiresTable);
//       }
//
//       // Check that all ops reference same DB.
//       let db_name = get_db_name(api.qualified_name());
//       if idx == 0 {
//         db = (db_name.to_string(), Some(api.conn().clone()));
//       } else if db_name != db.0 {
//         return Err(RecordError::BadRequest("ops can only touch same db"));
//       }
//
//       return Ok(api);
//     };
//
//   let deferred_ops = ops
//     .into_iter()
//     .enumerate()
//     .map(|(idx, op)| -> Result<Box<Op<T>>, RecordError> {
//       return match op {
//         Operation::Create { api_name, value } => {
//           let api = get_api(&state, &api_name, idx)?;
//           let mut record = extract_record(value)?;
//
//           if api.insert_autofill_missing_user_id_columns()
//             && let Some(user) = user
//           {
//             for column_index in api.user_id_columns() {
//               let col_name = &api.columns()[*column_index].column.name;
//               if !record.contains_key(col_name) {
//                 record.insert(
//                   col_name.to_owned(),
//                   serde_json::Value::String(uuid_to_b64(&user.uuid)),
//                 );
//               }
//             }
//           }
//
//           let mut lazy_params =
//             LazyParams::for_insert(&api, state.json_schema_registry().clone(), record, None);
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Create,
//             None,
//             Some(&mut lazy_params),
//             user,
//           )?;
//
//           let (query, _files) = WriteQuery::new_insert(
//             api.table_name(),
//             &api.record_pk_column().column.name,
//             api.insert_conflict_resolution_strategy(),
//             lazy_params
//               .consume()
//               .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let result = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(Some(
//               extract_record_id(result.pk_value.expect("insert"))
//                 .map_err(|err| RecordError::Internal(err.into()))?,
//             ));
//           }))
//         }
//         Operation::Update {
//           api_name,
//           record_id,
//           value,
//         } => {
//           let api = get_api(&state, &api_name, idx)?;
//           let record = extract_record(value)?;
//           let record_id = api.primary_key_to_value(record_id)?;
//           let pk_meta = api.record_pk_column();
//
//           let mut lazy_params = LazyParams::for_update(
//             &api,
//             state.json_schema_registry().clone(),
//             record,
//             None,
//             pk_meta.column.name.clone(),
//             record_id.clone(),
//           );
//
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Update,
//             Some(&record_id),
//             Some(&mut lazy_params),
//             user,
//           )?;
//
//           let (query, _files) = WriteQuery::new_update(
//             api.table_name(),
//             lazy_params
//               .consume()
//               .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let _ = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(None);
//           }))
//         }
//         Operation::Delete {
//           api_name,
//           record_id,
//         } => {
//           let api = get_api(&state, &api_name, idx)?;
//           let record_id = api.primary_key_to_value(record_id)?;
//
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Delete,
//             Some(&record_id),
//             None,
//             user,
//           )?;
//
//           let query = WriteQuery::new_delete(
//             api.table_name(),
//             &api.record_pk_column().column.name,
//             record_id,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let _ = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(None);
//           }))
//         }
//       };
//     })
//     .collect::<Result<Vec<_>, _>>()?;
//
//   if let Some(conn) = db.1 {
//     return Ok((conn, deferred_ops));
//   }
//
//   return Err(RecordError::BadRequest("empty ops?"));
// }
//
// fn build_deferred_ops2<T: SyncConnectionTrait + 'static>(
//   state: &AppState,
//   user: Option<&User>,
//   api: &RecordApi,
//   ops: Vec<Operation>,
// ) -> Result<Vec<Box<Op<T>>>, RecordError> {
//   let expected_db_name = get_db_name(api.qualified_name());
//
//   return ops
//     .into_iter()
//     .map(|op| -> Result<Box<Op<T>>, RecordError> {
//       return match op {
//         Operation::Create { api_name, value } => {
//           let api = get_api(&state, &api_name)?;
//           if get_db_name(api.qualified_name()) != expected_db_name {
//             return Err(RecordError::BadRequest("DB mismatch"));
//           }
//
//           let mut record = extract_record(value)?;
//
//           if api.insert_autofill_missing_user_id_columns()
//             && let Some(user) = user
//           {
//             for column_index in api.user_id_columns() {
//               let col_name = &api.columns()[*column_index].column.name;
//               if !record.contains_key(col_name) {
//                 record.insert(
//                   col_name.to_owned(),
//                   serde_json::Value::String(uuid_to_b64(&user.uuid)),
//                 );
//               }
//             }
//           }
//
//           let mut lazy_params =
//             LazyParams::for_insert(&api, state.json_schema_registry().clone(), record, None);
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Create,
//             None,
//             Some(&mut lazy_params),
//             user,
//           )?;
//
//           let (query, _files) = WriteQuery::new_insert(
//             api.table_name(),
//             &api.record_pk_column().column.name,
//             api.insert_conflict_resolution_strategy(),
//             lazy_params
//               .consume()
//               .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let result = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(Some(
//               extract_record_id(result.pk_value.expect("insert"))
//                 .map_err(|err| RecordError::Internal(err.into()))?,
//             ));
//           }))
//         }
//         Operation::Update {
//           api_name,
//           record_id,
//           value,
//         } => {
//           let api = get_api(&state, &api_name)?;
//           if get_db_name(api.qualified_name()) != expected_db_name {
//             return Err(RecordError::BadRequest("DB mismatch"));
//           }
//
//           let record = extract_record(value)?;
//           let record_id = api.primary_key_to_value(record_id)?;
//           let pk_meta = api.record_pk_column();
//
//           let mut lazy_params = LazyParams::for_update(
//             &api,
//             state.json_schema_registry().clone(),
//             record,
//             None,
//             pk_meta.column.name.clone(),
//             record_id.clone(),
//           );
//
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Update,
//             Some(&record_id),
//             Some(&mut lazy_params),
//             user,
//           )?;
//
//           let (query, _files) = WriteQuery::new_update(
//             api.table_name(),
//             lazy_params
//               .consume()
//               .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let _ = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(None);
//           }))
//         }
//         Operation::Delete {
//           api_name,
//           record_id,
//         } => {
//           let api = get_api(&state, &api_name)?;
//           if get_db_name(api.qualified_name()) != expected_db_name {
//             return Err(RecordError::BadRequest("DB mismatch"));
//           }
//
//           let record_id = api.primary_key_to_value(record_id)?;
//
//           let acl_check = api.build_deferred_record_level_access_check(
//             Permission::Delete,
//             Some(&record_id),
//             None,
//             user,
//           )?;
//
//           let query = WriteQuery::new_delete(
//             api.table_name(),
//             &api.record_pk_column().column.name,
//             record_id,
//           )
//           .map_err(|err| RecordError::Internal(err.into()))?;
//
//           Ok(Box::new(move |tx| {
//             acl_check(tx)?;
//
//             let _ = query
//               .apply_sync(tx)
//               .map_err(|err| RecordError::Internal(err.into()))?;
//
//             return Ok(None);
//           }))
//         }
//       };
//     })
//     .collect::<Result<Vec<_>, _>>();
// }

fn apply_ops<T: SyncConnectionTrait>(
  state: &AppState,
  conn: &T,
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
          let api = get_api(&state, &api_name)?;
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

          let (query, _files) = WriteQuery::new_insert(
            api.table_name(),
            &api.record_pk_column().column.name,
            api.insert_conflict_resolution_strategy(),
            lazy_params
              .consume()
              .map_err(|_| RecordError::BadRequest("Invalid Parameters"))?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let result = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Some(
            extract_record_id(result.pk_value.expect("insert"))
              .map_err(|err| RecordError::Internal(err.into()))?,
          ))
        }
        Operation::Update {
          api_name,
          record_id,
          value,
        } => {
          let api = get_api(&state, &api_name)?;
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
          let api = get_api(&state, &api_name)?;
          if get_db_name(api.qualified_name()) != expected_db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let record_id = api.primary_key_to_value(record_id)?;

          api.record_level_access_check(conn, Permission::Delete, Some(&record_id), None, user)?;

          let query = WriteQuery::new_delete(
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

    state.rebuild_connection_metadata().await.unwrap();

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
        transaction: None,
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
        transaction: None,
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
