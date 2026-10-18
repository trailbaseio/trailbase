use axum::extract::{Json, Path, Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use base64::prelude::*;
use futures_util::future::{BoxFuture, FutureExt, join_all};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use trailbase_schema::json_schema::FileUploadInput;
use trailbase_sqlite::traits::SyncTransaction;
use utoipa::{IntoParams, ToSchema};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::config::proto;
use crate::extract::Either;
use crate::records::files::{AsyncFileManager, delete_files_marked_for_deletion};
use crate::records::params::{FileMetadataContents, InsertParams, JsonRow, ParamsError};
use crate::records::transaction::{Op, OpResult};
use crate::records::write_queries::{InsertOrReplaceRecordApiQuery, WriteQuery};
use crate::records::{RecordApi, RecordError};
use crate::util::uuid_to_b64;

#[derive(Clone, Debug, Default, Deserialize, IntoParams)]
pub struct CreateRecordQuery {
  /// Redirect user to this address upon successful record creation.
  /// This only exists to support insertions via static HTML form actions.
  ///
  /// We may want to have a different on-error redirect to better support the static HTML use-case.
  pub redirect_uri: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateRecordResponse {
  /// Url-Safe base64 encoded ids of the newly created record.
  ///
  /// TODO: When conflicting inserts are ignored, unlike for transactions, we currently return an
  /// awkward "skipped" id. Maybe we should just remove batch inserts in favor of transactions or at
  /// least have the clients call the transaction endpoint instead.
  pub ids: Vec<String>,
}

#[inline]
pub(crate) fn extract_record(value: serde_json::Value) -> Result<JsonRow, RecordError> {
  return Ok(match value {
    serde_json::Value::Object(record) => record,
    _ => {
      return Err(RecordError::BadRequest("Expected single record"));
    }
  });
}

pub(crate) type RecordAndFiles = (JsonRow, Option<Vec<FileUploadInput>>);

#[inline]
fn extract_records(value: serde_json::Value) -> Result<Vec<RecordAndFiles>, RecordError> {
  return match value {
    serde_json::Value::Object(record) => Ok(vec![(record, None)]),
    serde_json::Value::Array(records) => {
      if records.len() > 1024 {
        return Err(RecordError::BadRequest("Bulk creation exceeds limit: 1024"));
      }

      records
        .into_iter()
        .map(|record| {
          let serde_json::Value::Object(record) = record else {
            return Err(RecordError::BadRequest(
              "Expected record or array of records",
            ));
          };

          return Ok((record, None));
        })
        .collect()
    }
    _ => Err(RecordError::BadRequest(
      "Expected record or array of records",
    )),
  };
}

/// Create new record.
#[utoipa::path(
  post,
  path = "/{name}",
  tag = "records",
  params(CreateRecordQuery),
  request_body = serde_json::Value,
  responses(
    (status = 200, description = "Ids of successfully created records.", body = CreateRecordResponse),
  )
)]
pub async fn create_record_handler(
  State(state): State<AppState>,
  Path(api_name): Path<String>,
  Query(create_record_query): Query<CreateRecordQuery>,
  user: Option<User>,
  either_request: Either<serde_json::Value>,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };
  if !api.is_table() {
    return Err(RecordError::ApiRequiresTable);
  }

  let mut records_and_files: Vec<RecordAndFiles> = match either_request {
    Either::Json(value) => extract_records(value)?,
    Either::Multipart(value, files) => vec![(extract_record(value)?, Some(files))],
    Either::Form(value) => vec![(extract_record(value)?, None)],
  };

  if records_and_files.is_empty() {
    return Err(RecordError::BadRequest("no values provided"));
  }

  let conn = api.conn();
  let conflict_resolution_strategy = api
    .insert_conflict_resolution_strategy()
    .unwrap_or(crate::config::proto::ConflictResolutionStrategy::Undefined);

  let record_ids: Vec<String> = match records_and_files.len() {
    1 => {
      let (record, files) = records_and_files.swap_remove(0);

      let (params, files) = build_insert_params(&state, &api, user.as_ref(), record, files)
        .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?;

      let (file_manager, file_committer) = if !files.is_empty() {
        let (file_manager, file_committer) =
          AsyncFileManager::new(state.objectstore().clone(), files);
        (Some(file_manager), Some(file_committer))
      } else {
        (None, None)
      };

      let query = InsertOrReplaceRecordApiQuery::new(api.clone(), params)?;
      query.check_table_level_access(user.as_ref())?;

      let write_result = conn
        .call_writer(move |mut conn| -> Result<_, RecordError> {
          query.check_record_level_access(&mut conn, user.as_ref())?;

          // We're storing any files to the object store first to make sure the DB entry is valid right
          // after commit and not racily pointing to soon-to-be-written files.
          if let Some(file_committer) = file_committer {
            file_committer.start_async_write();
          }

          return Ok(query.apply(&mut conn)?);
        })
        .await?;

      if let Some(mut file_manager) = file_manager {
        // Successful insert, release and do not cleanup written files.
        file_manager.release().await;

        if conflict_resolution_strategy == proto::ConflictResolutionStrategy::Replace {
          tokio::spawn(async move {
            let table_name = api.table_name().clone();
            if let Err(err) = delete_files_marked_for_deletion(
              api.conn(),
              state.objectstore(),
              &table_name,
              &[write_result.rowid],
            )
            .await
            {
              log::error!("Failed to delete files: {err}");
            }
          });
        }
      }

      vec![extract_record_id(write_result.pk_value.unwrap_or(
        trailbase_sqlite::Value::Integer(write_result.rowid),
      ))?]
    }
    _ => {
      let ops = records_and_files
        .into_iter()
        .map(|(record, files)| -> Result<_, RecordError> {
          let (params, files) = build_insert_params(&state, &api, user.as_ref(), record, files)
            .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?;

          let query = WriteQuery::new_insert_or_replace(api.clone(), params)?;

          Ok(Op {
            api: api.clone(),
            query,
            file_handling: if files.is_empty() {
              None
            } else {
              Some(AsyncFileManager::new(state.objectstore().clone(), files))
            },
          })
        })
        .collect::<Result<Vec<_>, _>>()?;

      // We use a transaction here so that if any of the ACL checks or writes fails, the entire
      // write will fail.
      let op_results = conn
        .transaction(move |mut tx| -> Result<Vec<OpResult>, RecordError> {
          let results = crate::records::transaction::apply_ops(&mut tx, user.as_ref(), ops)?;

          tx.commit()?;

          return Ok(results);
        })
        .await?;

      // TODO: We currently do not trigger file deletions eagerly and rely on periodic cleanup. We may
      // want to revisit.
      let results = join_all(op_results.into_iter().map(
        |op_result| -> BoxFuture<'static, Result<String, RecordError>> {
          return match op_result {
            OpResult::Ok { id, file_manager } => {
              if let Some(mut file_manager) = file_manager {
                return Box::pin(async move {
                  file_manager.release().await;
                  return extract_record_id(id);
                });
              }

              std::future::ready(extract_record_id(id)).boxed()
            }
            OpResult::IgnoredConflict => std::future::ready(Ok("skipped".to_string())).boxed(),
          };
        },
      ))
      .await;

      results.into_iter().collect::<Result<Vec<_>, _>>()?
    }
  };

  if let Some(redirect_uri) = create_record_query.redirect_uri {
    return Ok(Redirect::to(&redirect_uri).into_response());
  }

  return Ok(Json(CreateRecordResponse { ids: record_ids }).into_response());
}

pub(crate) fn build_insert_params(
  state: &AppState,
  api: &RecordApi,
  user: Option<&User>,
  mut record: JsonRow,
  files: Option<Vec<FileUploadInput>>,
) -> Result<(InsertParams, FileMetadataContents), ParamsError> {
  // Optionally insert user.
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

  return InsertParams::new(
    api,
    &state.json_schema_registry().load_full(),
    record,
    files,
  );
}

#[inline]
pub(crate) fn extract_record_id(value: trailbase_sqlite::Value) -> Result<String, RecordError> {
  return match value {
    trailbase_sqlite::Value::Blob(blob) => Ok(BASE64_URL_SAFE.encode(blob)),
    trailbase_sqlite::Value::Text(text) => Ok(text),
    trailbase_sqlite::Value::Integer(i) => Ok(i.to_string()),
    _ => Err(RecordError::Internal("Unexpected data type".into())),
  };
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::admin::user::create_user::create_user_for_test;
  use crate::app_state::*;
  use crate::auth::util::login_with_password;
  use crate::config::proto::{ConflictResolutionStrategy, PermissionFlag, RecordApiConfig};
  use crate::records::test_utils::*;
  use crate::records::*;
  use crate::test::unpack_json_response;
  use crate::util::{id_to_b64, uuid_to_b64};

  use serde_json::json;
  use trailbase_sqlite::params;

  #[tokio::test]
  async fn test_simple_record_api_create() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE simple (
            owner   {uuid} PRIMARY KEY CHECK(is_uuid(owner)) REFERENCES _user,
            value   INTEGER
          ) {strict};
        "#,
        strict = strict(conn),
        uuid = uuid_column(conn),
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("simple_api".to_string()),
        table_name: Some("simple".to_string()),
        conflict_resolution: Some(ConflictResolutionStrategy::Replace as i32),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        create_access_rule: Some("_USER_.id = _REQ_.owner".to_string()),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let password = "Secret!1!!";
    let user_x_email = "user_x@bar.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    // Test conflict resolution strategy replacement.
    for idx in 5..10 {
      let _ = create_record_handler(
        State(state.clone()),
        Path("simple_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(
          json_row_from_value(json!({
            "owner": id_to_b64(&user_x),
            "value": idx,
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .unwrap();
    }

    assert_eq!(
      state
        .conn()
        .read_query_row_get::<i64>(
          "SELECT value FROM simple WHERE owner = $1",
          params!(user_x),
          0
        )
        .await
        .unwrap(),
      Some(9)
    );

    {
      // Make sure user.id == owner ACL check works
      let response = create_record_handler(
        State(state.clone()),
        Path("simple_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(
          json_row_from_value(json!({
            "owner": uuid_to_b64(&uuid::Uuid::new_v4()),
            "value": 17,
          }))
          .unwrap()
          .into(),
        ),
      )
      .await;

      assert!(response.is_err());
    }
  }

  #[tokio::test]
  async fn test_record_api_create() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room = add_room(conn, "room0").await.unwrap();
    let password = "Secret!1!!";

    // Register message table as api.
    add_record_api(
      &state,
      "messages_api",
      "message",
      Acls {
        authenticated: vec![PermissionFlag::Create, PermissionFlag::Read],
        ..Default::default()
      },
      AccessRules {
        create: Some(
          "_USER_.id = _REQ_._owner AND EXISTS(SELECT 1 FROM room_members AS m WHERE m.user = _USER_.id AND m.room = _REQ_.room)".to_string(),
        ),
        ..Default::default()
      },
    )
    .await.unwrap();

    let user_x_email = "user_x@bar.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    add_user_to_room(conn, user_x, room).await.unwrap();

    let user_y_email = "user_y@test.com";
    let user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();

    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    {
      // User X can post to the room, they're a member of
      let json = json!({
        "_owner": id_to_b64(&user_x),
        "room": id_to_b64(&room),
        "data": "user_x message to room",
      });
      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(json_row_from_value(json).unwrap().into()),
      )
      .await;
      assert!(response.is_ok(), "{response:?}");

      let response: CreateRecordResponse = unpack_json_response(response.unwrap()).await.unwrap();

      assert_eq!(1, response.ids.len());
    }

    {
      // User X can bulk post to the room, they're a member of
      let json = |i: usize| {
        json!({
          "_owner": id_to_b64(&user_x),
          "room": id_to_b64(&room),
          "data": format!("user_x bulk message to room {i}"),
        })
      };

      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(serde_json::Value::Array(vec![json(0), json(1)])),
      )
      .await;
      assert!(response.is_ok(), "{response:?}");

      let response: CreateRecordResponse = unpack_json_response(response.unwrap()).await.unwrap();

      assert_eq!(2, response.ids.len());
    }

    {
      // User X cannot post as a different "_owner".
      let json = json!({
        "_owner": id_to_b64(&user_y),
        "room": id_to_b64(&room),
        "data": "user_x message to room",
      });
      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(json_row_from_value(json).unwrap().into()),
      )
      .await;
      assert!(response.is_err(), "{response:?}");
    }

    {
      // Bulk inserts are rolled back in a transaction is second insert fails.
      let count_before: i64 = state
        .conn()
        .read_query_row_get("SELECT COUNT(*) FROM message", (), 0)
        .await
        .unwrap()
        .unwrap();

      let json = |user_id: &[u8; 16]| {
        json!({
          "_owner": id_to_b64(user_id),
          "room": id_to_b64(&room),
          "data": "user_x bulk message to room",
        })
      };

      // This should fail because of user_y as _owner.
      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(serde_json::Value::Array(vec![json(&user_x), json(&user_y)])),
      )
      .await;
      assert!(response.is_err(), "{response:?}");

      let count_after: i64 = state
        .conn()
        .read_query_row_get("SELECT COUNT(*) FROM message", (), 0)
        .await
        .unwrap()
        .unwrap();

      assert_eq!(count_before, count_after);
    }

    {
      // User Y is not a member and cannot post to the room.
      let json = json!({
        "room": id_to_b64(&room),
        "data": "user_x message to room",
      });
      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_y_token.auth_token),
        Either::Json(json_row_from_value(json).unwrap().into()),
      )
      .await;
      assert!(response.is_err(), "{response:?}");
    }
  }

  #[tokio::test]
  async fn test_record_api_create_integer_id() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables_integer(&state)
      .await
      .unwrap();
    let room = add_room(conn, "room0").await.unwrap();
    let password = "Secret!1!!";

    // Register message table as api.
    add_record_api(
      &state,
      "messages_api",
      "message",
      Acls {
        authenticated: vec![PermissionFlag::Create, PermissionFlag::Read],
        ..Default::default()
      },
      AccessRules {
        create: Some(
          "_USER_.id = _REQ_._owner AND EXISTS(SELECT 1 FROM room_members AS m WHERE m.user = _USER_.id AND m.room = _REQ_.room)".to_string(),
        ),
        ..Default::default()
      },
    )
    .await.unwrap();

    let user_x_email = "user_x@bar.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    add_user_to_room(conn, user_x, room).await.unwrap();

    {
      // User X can post to the room, they're a member of
      let json = json!({
        "_owner": id_to_b64(&user_x),
        "room": id_to_b64(&room),
        "data": "user_x message to room",
      });
      let response = create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(json_row_from_value(json).unwrap().into()),
      )
      .await;

      assert!(response.is_ok(), "{response:?}");
    }
  }
}
