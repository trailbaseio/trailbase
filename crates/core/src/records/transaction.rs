use axum::extract::{Json, State};
use futures_util::future::{BoxFuture, FutureExt, join_all};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trailbase_schema::QualifiedName;
use trailbase_sqlite::traits::{SyncConnection, SyncTransaction};
use ts_rs::TS;
use utoipa::ToSchema;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::config::proto;
use crate::records::create_record::{build_insert_params, extract_record_id};
use crate::records::files::{AsyncFileCommitter, AsyncFileManager};
use crate::records::params::{ToParamsEnum, UpdateParams};
use crate::records::record_api::RecordApi;
use crate::records::write_queries::WriteQuery;
use crate::records::{Permission, RecordError};

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

pub(crate) struct Op {
  pub api: RecordApi,
  pub query: WriteQuery,
  pub file_handling: Option<(AsyncFileManager, AsyncFileCommitter)>,
}

pub(crate) enum OpResult {
  Ok {
    id: trailbase_sqlite::Value,
    file_manager: Option<AsyncFileManager>,
  },
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
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let (params, files) =
            build_insert_params(&state, &api, user.as_ref(), extract_record(value)?, None)
              .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?;

          let params = Arc::new(params);

          let query = WriteQuery::new_insert_or_replace(
            api.conn().connection_type(),
            api.table_name(),
            api.columns(),
            &api.record_pk_column().column.name,
            api
              .insert_conflict_resolution_strategy()
              .unwrap_or(crate::config::proto::ConflictResolutionStrategy::Undefined),
            params.clone(),
          )?;

          Ok(Op {
            api,
            query,
            file_handling: if files.is_empty() {
              None
            } else {
              Some(AsyncFileManager::new(state.objectstore().clone(), files))
            },
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
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let (params, files) = UpdateParams::new(
            &api,
            &state.json_schema_registry().load(),
            extract_record(value)?,
            None,
            api.record_pk_column().column.name.clone(),
            api.primary_key_to_value(record_id)?,
          )
          .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?;

          let params = Arc::new(params);

          let query = WriteQuery::new_update(
            api.conn().connection_type(),
            api.table_name(),
            params.clone(),
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Op {
            api,
            query,
            file_handling: if files.is_empty() {
              None
            } else {
              Some(AsyncFileManager::new(state.objectstore().clone(), files))
            },
          })
        }
        Operation::Delete {
          api_name,
          record_id,
        } => {
          let api = get_api(&state, &api_name)?;
          let db_name = get_db_name(api.qualified_name());
          if expected_db_name.get_or_insert_with(|| db_name.to_string()) != db_name {
            return Err(RecordError::BadRequest("DB mismatch"));
          }

          let query = WriteQuery::new_delete(
            api.conn().connection_type(),
            api.table_name(),
            &api.record_pk_column().column.name,
            api.primary_key_to_value(record_id)?,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(Op {
            api,
            query,
            file_handling: None,
          })
        }
      };
    })
    .collect::<Result<Vec<_>, _>>()?;

  let conn = ops[0].api.conn().clone();

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

  // TODO: We currently do not trigger file deletions eagerly and rely on periodic cleanup. We may
  // want to revisit.
  let results = join_all(op_results.into_iter().map(
    |op_result| -> BoxFuture<'static, TransactionResult> {
      return match op_result {
        OpResult::Ok { id, file_manager } => {
          if let Some(mut file_manager) = file_manager {
            return Box::pin(async move {
              file_manager.release().await;
              return TransactionResult::Id(extract_record_id(id).unwrap_or_default());
            });
          }

          std::future::ready(TransactionResult::Id(
            extract_record_id(id).unwrap_or_default(),
          ))
          .boxed()
        }
        OpResult::IgnoredConflict => {
          std::future::ready(TransactionResult::Error("INSERT CONFLICT".into())).boxed()
        }
      };
    },
  ))
  .await;

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
      let Op {
        api,
        query,
        file_handling,
      } = op;

      return match &query {
        WriteQuery::Insert { params, .. } => {
          api.sync_eager_check_record_level_access(
            conn,
            Permission::Create,
            None,
            params.params_enum(),
            user,
          )?;

          let conflict_resolution_strategy = api
            .insert_conflict_resolution_strategy()
            .unwrap_or(proto::ConflictResolutionStrategy::Undefined);

          let file_manager = if let Some((file_manager, file_committer)) = file_handling {
            file_committer.start_async_write();
            Some(file_manager)
          } else {
            None
          };

          match query.apply_sync(conn) {
            Ok(result) => Ok(OpResult::Ok {
              id: result
                .pk_value
                .unwrap_or(trailbase_sqlite::Value::Integer(result.rowid)),
              file_manager,
            }),
            // Skip over QueryReturnedNoRow errors for `Ignore` conflict strategy.
            Err(err)
              if conflict_resolution_strategy == proto::ConflictResolutionStrategy::Ignore
                && matches!(err, trailbase_sqlite::Error::QueryReturnedNoRows) =>
            {
              Ok(OpResult::IgnoredConflict)
            }
            Err(err) => Err(RecordError::Internal(err.into())),
          }
        }
        WriteQuery::Update { params, .. } => {
          api.sync_eager_check_record_level_access(
            conn,
            Permission::Update,
            Some(&params.pk_column_value),
            params.params_enum(),
            user,
          )?;

          let file_manager = if let Some((file_manager, file_committer)) = file_handling {
            file_committer.start_async_write();
            Some(file_manager)
          } else {
            None
          };

          let id = params.pk_column_value.clone();
          let _result = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(OpResult::Ok { id, file_manager })
        }
        WriteQuery::Delete {
          pk_value: record_id,
          ..
        } => {
          api.sync_eager_check_record_level_access(
            conn,
            Permission::Delete,
            Some(record_id),
            None,
            user,
          )?;

          let id = record_id.clone();
          let _result = query
            .apply_sync(conn)
            .map_err(|err| RecordError::Internal(err.into()))?;

          Ok(OpResult::Ok {
            id,
            file_manager: None,
          })
        }
      };
    })
    .collect();
}

#[cfg(test)]
mod tests {
  use std::assert_matches;

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
