use axum::{
  Json,
  extract::{Path, Query, State},
  response::Response,
};
use serde::Deserialize;
use trailbase_schema::FileUploads;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::records::expand::expand_tables;
use crate::records::expand::row_to_json_expand;
use crate::records::files::read_file_into_response;
use crate::records::params::SchemaAccessor;
use crate::records::read_queries::{
  ExpandedSelectQueryResult, run_expanded_select_query, run_get_file_query, run_get_files_query,
  run_select_query,
};
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
  path = "/{name}/{record}",
  tag = "records",
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

  let record_id = api.primary_key_to_value(record)?;

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

      let metadata = api.connection_metadata();
      let expanded_tables = expand_tables(&api, metadata, &query_expand)?;

      let Some(ExpandedSelectQueryResult { root, foreign_rows }) = run_expanded_select_query(
        api.conn(),
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
        let foreign_value = row_to_json_expand(
          &metadata.schema.columns,
          &metadata.json_metadata.columns,
          &row,
          prefix_filter,
          None,
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
      let Some(row) = run_select_query(
        api.conn(),
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
///
/// You may prefer using the more general "files" (plural) handler below. Since using unique
/// filenames does help with the content lifecycle, such as caching.
#[utoipa::path(
  get,
  path = "/{name}/{record}/file/{column_name}",
  tag = "records",
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

  let record_id = api.primary_key_to_value(record)?;

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

  let file_upload = run_get_file_query(
    api.conn(),
    api.table_name(),
    column,
    column_json_metadata,
    &pk_column.name,
    record_id,
  )
  .await?;

  return read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| RecordError::Internal(err.into()));
}

type GetUploadedFilesFromRecordPath = Path<(
  String, // RecordApi name
  String, // Record id
  String, // Column name
  // NOTE: We may want to remove index-based access in the future. A stable, unique identifier
  // makes a lot more sense in the context of mutations, caching, ... .
  String, // Filename
)>;

/// Read single file from list associated with record.
#[utoipa::path(
  get,
  path = "/{name}/{record}/files/{column_name}/{file_name}",
  tag = "records",
  responses(
    (status = 200, description = "File contents.")
  )
)]
pub async fn get_uploaded_files_from_record_handler(
  State(state): State<AppState>,
  Path((api_name, record, column_name, file_name)): GetUploadedFilesFromRecordPath,
  user: Option<User>,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.primary_key_to_value(record)?;
  api
    .check_record_level_access(Permission::Read, Some(&record_id), None, user.as_ref())
    .await?;

  let Some((_index, column, Some(column_json_metadata))) = api.column_by_name(&column_name) else {
    return Err(RecordError::BadRequest("Invalid field/column name"));
  };

  let FileUploads(file_uploads) = run_get_files_query(
    api.conn(),
    api.table_name(),
    column,
    column_json_metadata,
    &api.record_pk_column().1.name,
    record_id,
  )
  .await?;

  let file_upload = file_uploads
    .into_iter()
    .find(|f| f.filename() == file_name)
    .ok_or_else(|| RecordError::RecordNotFound)?;

  return read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| RecordError::Internal(err.into()));
}

#[inline]
fn prefix_filter(col_name: &str) -> bool {
  return !col_name.starts_with("_");
}

#[cfg(test)]
mod test {
  use std::io::Read;
  use std::sync::Arc;

  use axum::Json;
  use axum::extract::{Path, Query, State};
  use serde::Serialize;
  use serde_json::json;
  use trailbase_schema::{FileUpload, FileUploadInput};

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::user::User;
  use crate::auth::util::login_with_password;
  use crate::config::proto::{JsonSchemaConfig, PermissionFlag, RecordApiConfig};
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::records::create_record::{
    CreateRecordQuery, CreateRecordResponse, create_record_handler,
  };
  use crate::records::delete_record::delete_record_handler;
  use crate::records::params::JsonRow;
  use crate::records::test_utils::*;
  use crate::records::update_record::update_record_handler;
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

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(api_name.to_string()),
        table_name: Some("table".to_string()),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Delete as i32,
          PermissionFlag::Update as i32,
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
              name: Some("name".to_string()),
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

    let mut read_dir = tokio::fs::read_dir(state.data_dir().uploads_path())
      .await
      .unwrap();
    while let Some(entry) = read_dir.next_entry().await.unwrap() {
      // Fail if there's any entry and the file has not been deleted.
      panic!("File should be deleted: {entry:?}");
    }

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

  async fn read_objectstore_file(
    store: &Arc<ObjectStore>,
    path: &object_store::path::Path,
  ) -> Vec<u8> {
    let contents = store.get(&path).await.unwrap();

    let object_store::GetResultPayload::File(mut file, _path) = contents.payload else {
      panic!("expected file");
    };

    let mut buf = vec![];
    file.read_to_end(&mut buf).unwrap();
    return buf;
  }

  #[tokio::test]
  async fn test_multiple_file_upload_download_e2e_and_deletion() {
    let state = test_state(None).await.unwrap();
    const API_NAME: &str = "test_api";
    create_test_record_api(&state, API_NAME).await;

    let bytes0: Vec<u8> = vec![0, 1, 2, 3, 4, 5];
    let bytes1: Vec<u8> = vec![0, 1, 1, 2];
    let bytes2: Vec<u8> = vec![42, 5, 42, 5];

    let request = json!({
      "file" : FileUploadInput {
        name: Some("foo0".to_string()),
        filename: Some("bar0".to_string()),
        content_type: Some("baz0".to_string()),
        data: bytes0.clone(),
      },
      "files": vec![
          FileUploadInput {
            name: Some("foo1".to_string()),
            filename: Some("bar1".to_string()),
            content_type: Some("baz1".to_string()),
            data: bytes1.clone(),
          },
          FileUploadInput {
            name: Some("foo2".to_string()),
            filename: Some("bar2".to_string()),
            content_type: Some("baz2".to_string()),
            data: bytes2.clone(),
          },
      ],
    });

    let resp0: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(json_row_from_value(request.clone()).unwrap().into()),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let assert_all_files_contents = async |record_id: String| -> Vec<object_store::path::Path> {
      async fn assert_file(
        state: &AppState,
        index: i64,
        expected: &[u8],
        f: &FileUpload,
        read: impl AsyncFnOnce() -> Response,
      ) -> object_store::path::Path {
        assert_eq!(f.original_filename(), Some(format!("bar{index}").as_str()));
        assert_eq!(f.content_type(), Some(format!("baz{index}").as_str()));

        let file_path = object_store::path::Path::from(f.objectstore_id());
        assert_eq!(
          *expected,
          read_objectstore_file(state.objectstore(), &file_path).await
        );

        let response = read().await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
          .await
          .unwrap();

        assert_eq!(*expected, body);

        return file_path;
      }

      let Json(value) = read_record_handler(
        State(state.clone()),
        Path((API_NAME.to_string(), record_id.clone())),
        Query(ReadRecordQuery::default()),
        None,
      )
      .await
      .unwrap();

      let serde_json::Value::Object(map) = value else {
        panic!("Not a map");
      };

      let file: FileUpload = serde_json::from_value(map.get("file").unwrap().clone()).unwrap();
      let files: Vec<FileUpload> =
        serde_json::from_value(map.get("files").unwrap().clone()).unwrap();

      return vec![
        assert_file(&state, 0, &bytes0, &file, async || {
          return get_uploaded_file_from_record_handler(
            State(state.clone()),
            Path((API_NAME.to_string(), record_id.clone(), "file".to_string())),
            None,
          )
          .await
          .unwrap();
        })
        .await,
        assert_file(&state, 1, &bytes1, &files[0], async || {
          return get_uploaded_files_from_record_handler(
            State(state.clone()),
            Path((
              API_NAME.to_string(),
              record_id.clone(),
              "files".to_string(),
              files[0].filename().to_string(),
            )),
            None,
          )
          .await
          .unwrap();
        })
        .await,
        assert_file(&state, 2, &bytes2, &files[1], async || {
          return get_uploaded_files_from_record_handler(
            State(state.clone()),
            Path((
              API_NAME.to_string(),
              record_id.clone(),
              "files".to_string(),
              files[1].filename().to_string(),
            )),
            None,
          )
          .await
          .unwrap();
        })
        .await,
      ];
    };

    let paths0 = assert_all_files_contents.clone()(resp0.ids[0].clone()).await;

    // Insert two more records to check bulk creation.
    let resp1: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(serde_json::Value::Array(vec![
          request.clone(),
          request.clone(),
        ])),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let paths1_0 = assert_all_files_contents.clone()(resp1.ids[0].clone()).await;
    let paths1_1 = assert_all_files_contents.clone()(resp1.ids[1].clone()).await;

    for id in resp1.ids {
      let _ = delete_record_handler(State(state.clone()), Path((API_NAME.to_string(), id)), None)
        .await
        .unwrap();
    }

    // Update the first record, which will also trigger deletions.
    let _ = update_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), resp0.ids[0].clone())),
      None,
      Either::Json(json_row_from_value(request.clone()).unwrap().into()),
    )
    .await
    .unwrap();

    // Make sure the _file_deletions have been processed
    let count: i64 = state
      .conn()
      .read_query_value("SELECT COUNT(*) FROM _file_deletions", ())
      .await
      .unwrap()
      .unwrap();

    assert_eq!(0, count);

    // And the actual files are gone.
    for paths in [paths0, paths1_0, paths1_1] {
      for path in paths {
        if !matches!(
          state.objectstore().get(&path).await,
          Err(object_store::Error::NotFound { .. })
        ) {
          panic!("{path} should have been deleted");
        }
      }
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

    state.rebuild_connection_metadata().await.unwrap();

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

    state.rebuild_connection_metadata().await.unwrap();

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

    state.rebuild_connection_metadata().await.unwrap();

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

    state.rebuild_connection_metadata().await.unwrap();

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

  #[tokio::test]
  async fn test_custom_schema() {
    #[derive(Serialize, schemars::JsonSchema)]
    struct StringArray(Vec<String>);

    let config = {
      let mut config = test_config();

      config.schemas.push(JsonSchemaConfig {
        name: Some("StringArray".to_string()),
        schema: Some(serde_json::to_string_pretty(&schemars::schema_for!(StringArray)).unwrap()),
      });

      config
    };

    let state = test_state(Some(TestStateOptions {
      config: Some(config),
      ..Default::default()
    }))
    .await
    .unwrap();

    let name = "with_schema".to_string();

    state
      .conn()
      .execute(
        format!(
          r#"CREATE TABLE '{name}' (
            id           INTEGER PRIMARY KEY,
            list         TEXT NOT NULL CHECK(jsonschema('StringArray', list)) DEFAULT '[]'
          ) STRICT"#
        ),
        (),
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(name.clone()),
        table_name: Some(name.clone()),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Delete as i32,
          PermissionFlag::Update as i32,
        ]
        .into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let record = json!({
      "id": 1,
      "list": StringArray(vec!["item0".to_string(), "item1".to_string()]),
    });

    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(name.clone()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(record.clone()),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let Json(read_response) = read_record_handler(
      State(state),
      Path((name.clone(), create_response.ids[0].clone())),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    assert_eq!(read_response, record);
  }
}
