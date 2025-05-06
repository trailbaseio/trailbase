use axum::{
  Json,
  extract::{Path, Query, State},
  response::Response,
};
use serde::Deserialize;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::records::files::read_file_into_response;
use crate::records::query_builder::{
  ExpandedSelectQueryResult, GetFileQueryBuilder, GetFilesQueryBuilder, SelectQueryBuilder,
  expand_tables,
};
use crate::records::sql_to_json::{row_to_json, row_to_json_expand};
use crate::records::{Permission, RecordError};

#[derive(Debug, Default, Deserialize)]
pub struct ReadRecordQuery {
  /// Comma separated list of foreign key column names that should be expanded.
  ///
  /// Requires the API's configuration to explicitly allow expanding said columns.
  pub expand: Option<String>,
}

/// Read record.
#[utoipa::path(
  get,
  path = "/:name/:record",
  responses(
    (status = 200, description = "Record contents.", body = serde_json::Value)
  )
)]
pub async fn read_record_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  Query(query): Query<ReadRecordQuery>,
  user: Option<User>,
) -> Result<Json<serde_json::Value>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.id_to_sql(&record)?;

  api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await?;

  let (_index, pk_column) = api.record_pk_column();
  let column_names: Vec<_> = api.columns().iter().map(|c| c.name.as_str()).collect();

  return Ok(Json(match query.expand {
    Some(query_expand) if !query_expand.is_empty() => {
      let Some(expand) = api.expand() else {
        return Err(RecordError::BadRequest("Invalid expansion"));
      };

      // Input validation, i.e. only accept columns that are also configured.
      let query_expand: Vec<_> = query_expand.split(",").collect();
      for col_name in &query_expand {
        if !query_expand.contains(col_name) {
          return Err(RecordError::BadRequest("Invalid expansion"));
        }
      }

      let expanded_tables = expand_tables(
        state.schema_metadata(),
        |column_name| {
          api
            .column_index_by_name(column_name)
            .map(|idx| &api.columns()[idx])
        },
        &query_expand,
      )?;

      let Some(ExpandedSelectQueryResult { root, foreign_rows }) =
        SelectQueryBuilder::run_expanded(
          state.conn(),
          api.table_name(),
          &column_names,
          &pk_column.name,
          record_id,
          &expanded_tables,
        )
        .await?
      else {
        return Err(RecordError::RecordNotFound);
      };

      // Alloc a map from column name to value that's pre-filled with with Value::Null for all
      // expandable columns.
      let mut expand = expand.clone();

      for (col_name, (metadata, row)) in std::iter::zip(query_expand, foreign_rows) {
        let foreign_value = row_to_json(
          &metadata.schema.columns,
          &metadata.json_metadata.columns,
          &row,
          prefix_filter,
        )
        .map_err(|err| RecordError::Internal(err.into()))?;

        let result = expand.insert(col_name.to_string(), foreign_value);
        assert!(result.is_some());
      }

      row_to_json_expand(
        api.columns(),
        api.json_column_metadata(),
        &root,
        prefix_filter,
        Some(&expand),
      )
      .map_err(|err| RecordError::Internal(err.into()))?
    }
    Some(_) | None => {
      let Some(row) = SelectQueryBuilder::run(
        state.conn(),
        api.table_name(),
        &column_names,
        &pk_column.name,
        record_id,
      )
      .await?
      else {
        return Err(RecordError::RecordNotFound);
      };

      row_to_json_expand(
        api.columns(),
        api.json_column_metadata(),
        &row,
        prefix_filter,
        api.expand(),
      )
      .map_err(|err| RecordError::Internal(err.into()))?
    }
  }));
}

type GetUploadedFileFromRecordPath = Path<(
  String, // RecordApi name
  String, // Record id
  String, // Column name,
)>;

/// Read file associated with record.
#[utoipa::path(
  get,
  path = "/:name/:record/file/:column_name",
  responses(
    (status = 200, description = "File contents.")
  )
)]
pub async fn get_uploaded_file_from_record_handler(
  state: State<AppState>,
  Path((api_name, record, column_name)): GetUploadedFileFromRecordPath,
  user: Option<User>,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.id_to_sql(&record)?;

  let Ok(()) = api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await
  else {
    return Err(RecordError::Forbidden);
  };

  let (_index, pk_column) = api.record_pk_column();
  let Some(index) = api.column_index_by_name(&column_name) else {
    return Err(RecordError::BadRequest("Invalid field/column name"));
  };

  let column = &api.columns()[index];
  let Some(ref column_json_metadata) = api.json_column_metadata()[index] else {
    return Err(RecordError::BadRequest("Invalid column"));
  };

  let file_upload = GetFileQueryBuilder::run(
    &state,
    api.table_name(),
    column,
    column_json_metadata,
    &pk_column.name,
    record_id,
  )
  .await
  .map_err(|err| RecordError::Internal(err.into()))?;

  return read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| RecordError::Internal(err.into()));
}

type GetUploadedFilesFromRecordPath = Path<(
  String, // RecordApi name
  String, // Record id
  String, // Column name,
  usize,  // Index
)>;

/// Read single file from list associated with record.
#[utoipa::path(
  get,
  path = "/:name/:record/files/:column_name/:file_index",
  responses(
    (status = 200, description = "File contents.")
  )
)]
pub async fn get_uploaded_files_from_record_handler(
  State(state): State<AppState>,
  Path((api_name, record, column_name, file_index)): GetUploadedFilesFromRecordPath,
  user: Option<User>,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.id_to_sql(&record)?;

  let Ok(()) = api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await
  else {
    return Err(RecordError::Forbidden);
  };

  let (_index, pk_column) = api.record_pk_column();
  let Some(index) = api.column_index_by_name(&column_name) else {
    return Err(RecordError::BadRequest("Invalid field/column name"));
  };

  let column = &api.columns()[index];
  let Some(ref column_json_metadata) = api.json_column_metadata()[index] else {
    return Err(RecordError::BadRequest("Invalid column"));
  };

  let mut file_uploads = GetFilesQueryBuilder::run(
    &state,
    api.table_name(),
    column,
    column_json_metadata,
    &pk_column.name,
    record_id,
  )
  .await
  .map_err(|err| RecordError::Internal(err.into()))?;

  if file_index >= file_uploads.0.len() {
    return Err(RecordError::RecordNotFound);
  }

  return read_file_into_response(&state, file_uploads.0.remove(file_index))
    .await
    .map_err(|err| RecordError::Internal(err.into()));
}

#[inline]
fn prefix_filter(col_name: &str) -> bool {
  return !col_name.starts_with("_");
}

#[cfg(test)]
mod test {
  use axum::Json;
  use axum::extract::{Path, Query, State};
  use serde_json::json;
  use trailbase_schema::{FileUpload, FileUploadInput};

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::User;
  use crate::config::proto::{PermissionFlag, RecordApiConfig};
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::records::create_record::{
    CreateRecordQuery, CreateRecordResponse, create_record_handler,
  };
  use crate::records::delete_record::delete_record_handler;
  use crate::records::params::JsonRow;
  use crate::records::test_utils::*;
  use crate::test::unpack_json_response;
  use crate::util::id_to_b64;

  #[tokio::test]
  async fn ignores_extra_sql_parameters_test() {
    // This test is actually just testing our SQL driver and making sure that we can overprovision
    // arguments. Specifically, we want to provide :user and :id arguments even if they're not
    // consumed by a user-provided access query.
    let state = test_state(None).await.unwrap();
    let conn = state.user_conn();

    const EMAIL: &str = "foo@bar.baz";
    conn
      .execute(
        format!(r#"INSERT INTO "{USER_TABLE}" (email) VALUES ($1)"#),
        trailbase_sqlite::params!(EMAIL),
      )
      .await
      .unwrap();

    let count: i64 = conn
      .read_query_row_f(
        format!(r#"SELECT COUNT(*) from "{USER_TABLE}" WHERE email = :email"#),
        trailbase_sqlite::named_params! {
          ":email": EMAIL,
          ":unused": "unused",
          ":foo": 42,
        },
        |row| row.get(0),
      )
      .await
      .unwrap()
      .unwrap();

    assert_eq!(1, count);
  }

  #[tokio::test]
  async fn test_record_api_read() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    // Add tables and record api before inserting data.
    create_chat_message_app_tables(&state).await.unwrap();
    let room0 = add_room(conn, "room0").await.unwrap();
    let room1 = add_room(conn, "room1").await.unwrap();
    let password = "Secret!1!!";

    // Register message table as record api with moderator read access.
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("messages_api".to_string()),
        table_name: Some("message".to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        read_access_rule: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND user = _USER_.id))".to_string()),
        ..Default::default()
      },
    ).await.unwrap();

    let user_x_email = "user_x@test.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();

    add_user_to_room(conn, user_x, room0).await.unwrap();
    add_user_to_room(conn, user_x, room1).await.unwrap();

    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    let user_y_email = "user_y@foo.baz";
    let user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();

    add_user_to_room(conn, user_y, room0).await.unwrap();

    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    // Finally, create some messages and try to access them.
    {
      // Post to room0. X, Y, and mod should be able to read it.
      let message_id = send_message(conn, user_x, room0, "from user_x to room0")
        .await
        .unwrap();

      // No creds, no read
      assert!(
        read_record_handler(
          State(state.clone()),
          Path(("messages_api".to_string(), id_to_b64(&message_id),)),
          Query(ReadRecordQuery::default()),
          None
        )
        .await
        .is_err()
      );

      {
        // User X
        let response = read_record_handler(
          State(state.clone()),
          Path(("messages_api".to_string(), id_to_b64(&message_id))),
          Query(ReadRecordQuery::default()),
          User::from_auth_token(&state, &user_x_token.auth_token),
        )
        .await;
        assert!(response.is_ok(), "{response:?}");
      }

      {
        // User Y
        let response = read_record_handler(
          State(state.clone()),
          Path(("messages_api".to_string(), id_to_b64(&message_id))),
          Query(ReadRecordQuery::default()),
          User::from_auth_token(&state, &user_y_token.auth_token),
        )
        .await;
        assert!(response.is_ok(), "{response:?}");
      }
    }

    {
      // Post to room1. Only X, and mod should be able to read it. User Y is not a member
      let message_id = send_message(conn, user_x, room1, "from user_x to room1")
        .await
        .unwrap();

      // User Y
      let response = read_record_handler(
        State(state.clone()),
        Path(("messages_api".to_string(), id_to_b64(&message_id))),
        Query(ReadRecordQuery::default()),
        User::from_auth_token(&state, &user_y_token.auth_token),
      )
      .await;
      assert!(response.is_err(), "{response:?}");
    }
  }

  async fn create_test_record_api(state: &AppState, api_name: &str) {
    let conn = state.conn();
    conn
      .execute(
        format!(
          r#"CREATE TABLE 'table' (
            id           BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
            file         TEXT CHECK(jsonschema('std.FileUpload', file)),
            files        TEXT CHECK(jsonschema('std.FileUploads', files)),
            -- Add a "keyword" column to ensure escaping is correct
            [index]      TEXT NOT NULL DEFAULT('')
          ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(api_name.to_string()),
        table_name: Some("table".to_string()),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Delete as i32,
        ]
        .into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();
  }

  // NOTE: would ideally be in a create_record test instead.
  #[tokio::test]
  async fn test_empty_create_record() {
    let state = test_state(None).await.unwrap();

    const API_NAME: &str = "test_api";
    create_test_record_api(&state, API_NAME).await;

    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(JsonRow::new().into()),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let record_path = (API_NAME.to_string(), create_response.ids[0].clone());

    let Json(_) = read_record_handler(
      State(state),
      Path(record_path),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();
  }

  #[tokio::test]
  async fn test_escaping_keywords_for_create_record() {
    const API_NAME: &str = "table";
    let state = test_state(None).await.unwrap();
    create_test_record_api(&state, API_NAME).await;

    let column_value = "test";
    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(
          json_row_from_value(json!({
            "index": column_value.to_string(),
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let record_path = (API_NAME.to_string(), create_response.ids[0].clone());

    let Json(value) = read_record_handler(
      State(state),
      Path(record_path),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let serde_json::Value::Object(map) = value else {
      panic!("Not a map");
    };

    assert_eq!(
      *map.get("index").unwrap(),
      serde_json::Value::String(column_value.to_string())
    );
  }

  #[tokio::test]
  async fn test_single_file_upload_download_e2e() {
    let state = test_state(None).await.unwrap();
    const API_NAME: &str = "test_api";
    create_test_record_api(&state, API_NAME).await;

    let bytes: Vec<u8> = vec![42, 5, 42, 5];
    let file_column = "file";
    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(
          json_row_from_value(json!({
            file_column: FileUploadInput {
              name: Some("foo".to_string()),
              filename: Some("bar".to_string()),
              content_type: Some("baz".to_string()),
              data: bytes.clone(),
            },
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let record_path = (API_NAME.to_string(), create_response.ids[0].clone());

    let Json(value) = read_record_handler(
      State(state.clone()),
      Path(record_path.clone()),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let serde_json::Value::Object(map) = value else {
      panic!("Not a map");
    };

    let file_upload: FileUpload = serde_json::from_value(map.get("file").unwrap().clone()).unwrap();
    assert_eq!(file_upload.original_filename(), Some("bar"));
    assert_eq!(file_upload.content_type(), Some("baz"));

    let record_file_path = Path((
      API_NAME.to_string(),
      create_response.ids[0].clone(),
      file_column.to_string(),
    ));

    let read_response = get_uploaded_file_from_record_handler(
      State(state.clone()),
      Path(record_file_path.clone()),
      None,
    )
    .await
    .unwrap();

    let body = axum::body::to_bytes(read_response.into_body(), usize::MAX)
      .await
      .unwrap();
    assert_eq!(body.to_vec(), bytes);

    let _ = delete_record_handler(State(state.clone()), Path(record_path.clone()), None)
      .await
      .unwrap();

    let mut dir_cnt = 0;
    let mut read_dir = tokio::fs::read_dir(state.data_dir().uploads_path())
      .await
      .unwrap();
    while let Some(_entry) = read_dir.next_entry().await.unwrap() {
      dir_cnt += 1;
    }
    assert_eq!(dir_cnt, 0);

    assert!(
      get_uploaded_file_from_record_handler(
        State(state.clone()),
        Path(record_file_path.clone()),
        None,
      )
      .await
      .is_err()
    );
  }

  #[tokio::test]
  async fn test_multiple_file_upload_download_e2e() {
    let state = test_state(None).await.unwrap();
    const API_NAME: &str = "test_api";
    create_test_record_api(&state, API_NAME).await;

    let bytes1: Vec<u8> = vec![0, 1, 1, 2];
    let bytes2: Vec<u8> = vec![42, 5, 42, 5];

    let files_column = "files";
    let resp: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(
          json_row_from_value(json!({
            files_column: vec![
            FileUploadInput {
              name: Some("foo0".to_string()),
              filename: Some("bar0".to_string()),
              content_type: Some("baz0".to_string()),
              data: bytes1.clone(),
            },
            FileUploadInput {
              name: Some("foo1".to_string()),
              filename: Some("bar1".to_string()),
              content_type: Some("baz1".to_string()),
              data: bytes2.clone(),
            },
            ],
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let record_path = Path((API_NAME.to_string(), resp.ids[0].clone()));

    let Json(value) = read_record_handler(
      State(state.clone()),
      record_path,
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let serde_json::Value::Object(map) = value else {
      panic!("Not a map");
    };

    let file_uploads: Vec<FileUpload> =
      serde_json::from_value(map.get("files").unwrap().clone()).unwrap();

    for (index, bytes) in [(0, bytes1), (1, bytes2)] {
      let f = &file_uploads[index];
      assert_eq!(f.original_filename(), Some(format!("bar{index}").as_str()));
      assert_eq!(f.content_type(), Some(format!("baz{index}").as_str()));

      let record_file_path = Path((
        API_NAME.to_string(),
        resp.ids[0].clone(),
        files_column.to_string(),
        index,
      ));

      let response =
        get_uploaded_files_from_record_handler(State(state.clone()), record_file_path, None)
          .await
          .unwrap();

      let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
      assert_eq!(body.to_vec(), bytes);
    }
  }

  #[tokio::test]
  async fn test_read_record_from_view() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    // Add tables and record api before inserting data.
    create_chat_message_app_tables(&state).await.unwrap();

    // Create view
    let table_name = "message";
    let view_name = "view";
    conn
      .execute(
        format!("CREATE VIEW '{view_name}' AS SELECT * FROM {table_name}"),
        (),
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    let room0 = add_room(conn, "room0").await.unwrap();
    let room1 = add_room(conn, "room1").await.unwrap();
    let password = "Secret!1!!";

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("messages_api".to_string()),
        table_name: Some(view_name.to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        read_access_rule: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND user = _USER_.id))".to_string()),
        ..Default::default()
      },
    )
    .await.unwrap();

    let user_x_email = "user_x@test.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();

    add_user_to_room(conn, user_x, room0).await.unwrap();
    add_user_to_room(conn, user_x, room1).await.unwrap();

    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    // Post to room0. X, Y, and mod should be able to read it.
    let message_id = send_message(conn, user_x, room0, "from user_x to room0")
      .await
      .unwrap();

    // User X
    let response = read_record_handler(
      State(state.clone()),
      Path(("messages_api".to_string(), id_to_b64(&message_id))),
      Query(ReadRecordQuery::default()),
      User::from_auth_token(&state, &user_x_token.auth_token),
    )
    .await;
    assert!(response.is_ok(), "{response:?}");
  }

  #[tokio::test]
  async fn test_record_api_with_excluded_columns() {
    let state = test_state(None).await.unwrap();

    const API_NAME: &str = "test_api";

    state
      .conn()
      .execute(
        format!(
          r#"CREATE TABLE 'table' (
            pid          INTEGER PRIMARY KEY,
            [drop]       TEXT NOT NULL,
            [index]      TEXT NOT NULL DEFAULT('')
          ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(API_NAME.to_string()),
        table_name: Some("table".to_string()),
        acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        excluded_columns: vec!["index".to_string()],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let value = json!({
      "pid": 1,
      "drop": "foo".to_string(),
    });

    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(json_row_from_value(value.clone()).unwrap().into()),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(create_response.ids[0], "1");

    let Json(json) = read_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), create_response.ids[0].clone())),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    assert_eq!(json, value);

    // Providing a value for the hidden column should be ignored
    create_record_handler(
      State(state.clone()),
      Path(API_NAME.to_string()),
      Query(CreateRecordQuery::default()),
      None,
      Either::Json(
        json_row_from_value(json!({
          "pid": 2,
          "drop": "foo".to_string(),
          "index": "INACCESSIBLE".to_string(),
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    let index: String = state
      .conn()
      .read_query_row_f(r#"SELECT "index" from "table" WHERE pid = 2"#, (), |row| {
        row.get(0)
      })
      .await
      .unwrap()
      .unwrap();
    assert_eq!(index, "");
  }

  #[tokio::test]
  async fn test_field_presence_acls() {
    const TABLE_NAME: &str = "table";
    const API_NAME: &str = "table";
    let state = test_state(None).await.unwrap();
    let conn = state.conn();
    conn
      .execute(
        format!(
          r#"CREATE TABLE '{TABLE_NAME}' (
             id           INTEGER PRIMARY KEY NOT NULL,
             col0         TEXT NOT NULL DEFAULT(''),
             col1         TEXT NOT NULL DEFAULT('')
           ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(API_NAME.to_string()),
        table_name: Some(TABLE_NAME.to_string()),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Update as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Delete as i32,
        ]
        .into(),
        create_access_rule: Some("('col0' NOT IN _REQ_FIELDS_)".to_string()),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    create_record_handler(
      State(state.clone()),
      Path(API_NAME.to_string()),
      Query(CreateRecordQuery::default()),
      None,
      Either::Json(
        json_row_from_value(json!({
          "col1": "value".to_string(),
          "NON_EXISTANT": "value".to_string(),
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    assert!(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(
          json_row_from_value(json!({
            "col0": "value".to_string(),
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .is_err()
    );
  }

  #[tokio::test]
  async fn test_expand_fields() {
    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE parent (
            id           INTEGER PRIMARY KEY NOT NULL,
            value        TEXT NOT NULL
          ) STRICT;
          INSERT INTO parent (id, value) VALUES (1, 'first'), (2, 'second');

          CREATE TABLE child (
            id           INTEGER PRIMARY KEY NOT NULL,
            parent       INTEGER REFERENCES parent NOT NULL
          ) STRICT;
          INSERT INTO child (id, parent) VALUES (1, 1), (2, 2);

          CREATE VIEW child_view AS SELECT * FROM child;
       "#,
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("child_api".to_string()),
        table_name: Some("child".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        expand: vec!["parent".to_string()],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let expected = json!({
      "id": 1,
      "parent": {
        "id": 1,
        "data": {
          "id": 1,
          "value":"first",
        },
      },
    });

    let Json(value) = read_record_handler(
      State(state.clone()),
      Path(("child_api".to_string(), "1".to_string())),
      Query(ReadRecordQuery {
        expand: Some("parent".to_string()),
      }),
      None,
    )
    .await
    .unwrap();

    assert_eq!(value, expected);

    // Test views.
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("child_view_api".to_string()),
        table_name: Some("child_view".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        expand: vec!["parent".to_string()],
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let Json(value) = read_record_handler(
      State(state.clone()),
      Path(("child_view_api".to_string(), "1".to_string())),
      Query(ReadRecordQuery {
        expand: Some("parent".to_string()),
      }),
      None,
    )
    .await
    .unwrap();

    assert_eq!(value, expected);
  }
}
