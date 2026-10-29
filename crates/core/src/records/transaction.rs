use axum::extract::{Json, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::QualifiedName;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::config::proto;
use crate::records::RecordError;
use crate::records::create_record::{build_insert_params, extract_record_id};
use crate::records::params::{ParamOptions, UpdateParams};
use crate::records::queries::{
  DeleteRecordApiQuery, InsertOrReplaceRecordApiQuery, UpdateRecordApiQuery,
};
use crate::records::record_api::RecordApi;

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
  /// A 1:1 mapping of operations to results.
  pub results: Vec<TransactionResult>,
}

pub(crate) enum WriteQuery {
  Insert(InsertOrReplaceRecordApiQuery),
  Update(UpdateRecordApiQuery),
  Delete(DeleteRecordApiQuery),
}

pub(crate) struct Op {
  pub api: RecordApi,
  pub query: WriteQuery,
}

pub(crate) enum OpResult {
  Ok { id: trailbase_sqlite::Value },
  IgnoredConflict,
}

/// Execute a batch of transactions.
#[utoipa::path(
  post,
  path = "/api/transaction/v1/execute",
  tag = "records",
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

  let mut expected_db_name: Option<String> = None;
  let ops = request
    .operations
    .into_iter()
    .map(|op| -> Result<Op, RecordError> {
      return match op {
        Operation::Create { api_name, value } => {
          let api = get_api(&state, &api_name)?;
          let db_name = get_db_name(api.qualified_name());
          if expected_db_name.get_or_insert_with(|| db_name.to_string()) != db_name {
            return Err(DB_MISMATCH_ERR);
          }

          let (params, files) =
            build_insert_params(&state, &api, user.as_ref(), extract_record(value)?, None)
              .map_err(|_err| INVALID_PARAMS_ERR)?;

          if !files.is_empty() {
            return Err(FILES_IN_BATCH_MODE_ERR);
          }

          Ok(Op {
            query: WriteQuery::Insert(InsertOrReplaceRecordApiQuery::new(api.clone(), params)?),
            api,
          })
        }
        Operation::Update {
          api_name,
          record_id,
          value,
        } => {
          let api = get_api(&state, &api_name)?;
          let db_name = get_db_name(api.qualified_name());
          if expected_db_name.get_or_insert_with(|| db_name.to_string()) != db_name {
            return Err(DB_MISMATCH_ERR);
          }

          let (params, files) = UpdateParams::new(
            &api,
            &state.json_schema_registry().load(),
            (
              &api.record_pk_column().column.name,
              api.primary_key_to_value(record_id)?,
            ),
            extract_record(value)?,
            ParamOptions {
              multipart_files: None,
              file_token_decoder: None,
            },
          )
          .map_err(|_err| INVALID_PARAMS_ERR)?;

          if !files.is_empty() {
            return Err(FILES_IN_BATCH_MODE_ERR);
          }

          Ok(Op {
            query: WriteQuery::Update(UpdateRecordApiQuery::new(api.clone(), params)?),
            api,
          })
        }
        Operation::Delete {
          api_name,
          record_id,
        } => {
          let api = get_api(&state, &api_name)?;
          let db_name = get_db_name(api.qualified_name());
          if expected_db_name.get_or_insert_with(|| db_name.to_string()) != db_name {
            return Err(DB_MISMATCH_ERR);
          }

          Ok(Op {
            query: WriteQuery::Delete(DeleteRecordApiQuery::new(
              api.clone(),
              api.primary_key_to_value(record_id)?,
            )?),
            api,
          })
        }
      };
    })
    .collect::<Result<Vec<_>, _>>()?;

  let conn = ops[0].api.conn(user.as_ref().and_then(|u| u.tenant)).await;

  let op_results = if request.transaction.unwrap_or(false) {
    conn
      .transaction(move |mut tx| -> Result<_, RecordError> {
        let results = apply_ops(&mut tx, user.as_ref(), ops)?;

        tx.commit()?;

        return Ok(results);
      })
      .await?
  } else {
    conn
      .call_writer(move |mut conn| -> Result<_, RecordError> {
        return apply_ops(&mut conn, user.as_ref(), ops);
      })
      .await?
  };

  let results = op_results
    .into_iter()
    .map(|op_result| -> TransactionResult {
      return match op_result {
        OpResult::Ok { id } => TransactionResult::Id(extract_record_id(id).unwrap_or_default()),
        OpResult::IgnoredConflict => TransactionResult::Error("INSERT CONFLICT".into()),
      };
    })
    .collect();

  return Ok(Json(TransactionResponse { results }));
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
pub(crate) fn apply_ops(
  conn: &mut impl SyncConnection,
  user: Option<&User>,
  ops: Vec<Op>,
) -> Result<Vec<OpResult>, RecordError> {
  return ops
    .into_iter()
    .map(|op| -> Result<OpResult, RecordError> {
      let Op { api, query } = op;

      return match query {
        WriteQuery::Insert(query) => {
          query.check_table_level_access(user)?;
          query.check_record_level_access(conn, user)?;

          match query.apply(conn) {
            Ok(result) => Ok(OpResult::Ok {
              id: result
                .pk_value
                .unwrap_or(trailbase_sqlite::Value::Integer(result.rowid)),
            }),
            Err(err) => {
              let conflict_resolution_strategy = api
                .insert_conflict_resolution_strategy()
                .unwrap_or(crate::config::proto::ConflictResolutionStrategy::Undefined);

              if conflict_resolution_strategy == proto::ConflictResolutionStrategy::Ignore
                && matches!(err, trailbase_sqlite::Error::QueryReturnedNoRows)
              {
                // Skip over QueryReturnedNoRow errors for `Ignore` conflict strategy.
                Ok(OpResult::IgnoredConflict)
              } else {
                Err(RecordError::Internal(err.into()))
              }
            }
          }
        }
        WriteQuery::Update(query) => {
          query.check_table_level_access(user)?;
          query.check_record_level_access(conn, user)?;

          let _result = query.apply(conn)?;

          Ok(OpResult::Ok {
            id: query.pk_value(),
          })
        }
        WriteQuery::Delete(query) => {
          query.check_table_level_access(user)?;
          query.check_record_level_access(conn, user)?;

          let _result = query
            .apply(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(OpResult::Ok {
            id: query.pk_value(),
          })
        }
      };
    })
    .collect();
}

const DB_MISMATCH_ERR: RecordError = RecordError::BadRequest("DB mismatch");
const INVALID_PARAMS_ERR: RecordError = RecordError::BadRequest("Invalid Parameters");

/// We cannot support files efficiently (which is the point of batch mode) with strong consistency
/// in batch mode.
///
/// Both in batch and transaction mode, we'd have to await file-write success before committing an
/// operation, which would not work in transaction mode w/o locking the entire DB for extended
/// periods. In Batch mode we could break up execution into checking ACLs first, then writing files,
/// and then committing the DB operations, however ACL checks can depend on prior operations, which
/// makes this impossible.
///
/// We could also be more optimistic, e.g. disallow only tokens and "hope" content writes will go
/// through (e.g. no disk space or permission issues), however this may lead to invalid file
/// references and hard-to-debug issues.
///
/// In the end, one is probably be better off to just submit N independent operations.
pub(crate) const FILES_IN_BATCH_MODE_ERR: RecordError =
  RecordError::BadRequest("Files are not supported in batch-mode/transactions");

#[cfg(test)]
mod tests {
  use serde_json::json;
  use std::assert_matches;

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
        conflict_resolution: Some(proto::ConflictResolutionStrategy::Replace as i32),
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

    // Make sure invalid input gets mapped to BAD_REQUEST even though evaluated on the SQLite
    // executor.
    for tx in [true, false] {
      assert_matches!(
        record_transactions_handler(
          State(state.clone()),
          None,
          Json(TransactionRequest {
            operations: vec![Operation::Create {
              api_name: "test_api".to_string(),
              // Invalid input.
              value: serde_json::Value::Null,
            }],
            transaction: Some(tx),
          }),
        )
        .await,
        Err(RecordError::BadRequest(_))
      );
    }

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
        .read_query_row_get::<i64>("SELECT COUNT(*) FROM test;", (), 0)
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
        conflict_resolution: Some(proto::ConflictResolutionStrategy::Ignore as i32),
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
