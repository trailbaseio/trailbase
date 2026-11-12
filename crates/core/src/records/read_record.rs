use axum::extract::{Path, Query, State};
use axum::response::Response;
use compact_str::{CompactString, ToCompactString};
use serde::Deserialize;
use trailbase_schema::json::{record_to_json_expand, record_to_json_expand_ref};
use trailbase_schema::json_schema::FileUpload;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::extract::RawJson;
use crate::records::RecordError;
use crate::records::expand::build_expand_tables_params;
use crate::records::files::read_file_into_response;
use crate::records::queries::ReadFileRecordApiQuery;
use crate::records::queries::ReadRecordApiQuery;
use crate::schema_metadata::JsonColumnMetadata;

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
) -> Result<RawJson, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let record_id = api.primary_key_to_value(record)?;

  let config_expand = api.expand();
  let query_expand = query.expand.as_ref().map(|expand| {
    expand
      .split(",")
      .filter(|fragment| !fragment.is_empty())
      .collect::<Vec<_>>()
  });
  let expanded_tables = query_expand
    .map(|expanded_columns| {
      let metadata = api.connection_metadata();

      // This will drop any unknown expanded columns, thus also prevent any SQL template injection.
      for col_name in &expanded_columns {
        if !config_expand.iter().any(|c| *c == col_name) {
          return Err(RecordError::BadRequest("Invalid expansion"));
        }
      }

      return build_expand_tables_params(&api, metadata, &expanded_columns);
    })
    .transpose()?;

  let query = ReadRecordApiQuery::new(api.clone(), record_id.clone(), expanded_tables.as_deref())?;
  query.check_table_level_access(user.as_ref())?;

  let mut row = api
    .conn()
    .call_reader({
      move |mut conn| -> Result<_, RecordError> {
        query.check_record_level_access(&mut conn, user.as_ref())?;
        return query.read(&mut conn);
      }
    })
    .await?;

  let file_metadata_sanitizer: Option<&trailbase_schema::json::FileMetadataSanitizer> =
    if api.has_file_columns() {
      let table_name = api.table_name().clone();
      let record_id = record_id.clone();
      let file_token_key = *state.file_token_key();

      Some(&move |column_name: &str, metadata: FileUpload| {
        return crate::records::file_id::file_metadata_sanitizer(
          &file_token_key,
          &table_name,
          column_name,
          &record_id,
          metadata,
        );
      })
    } else {
      None
    };

  let json_obj = if let Some(expanded_tables) = expanded_tables
    && !expanded_tables.is_empty()
  {
    let mut curr = row.split_off(api.columns().len());

    let mut expand: Vec<(CompactString, _)> = Vec::with_capacity(expanded_tables.len());
    for expanded in &expanded_tables {
      let next = curr.split_off(expanded.num_columns);

      let foreign_value = record_to_json_expand(
        &expanded.metadata.column_metadata,
        &[],
        &curr,
        None,
        file_metadata_sanitizer,
      )
      .map_err(|err| RecordError::Internal(err.into()))?;

      expand.push((
        expanded.local_column_name.to_compact_string(),
        foreign_value,
      ));

      curr = next;
    }

    record_to_json_expand_ref(
      api.columns(),
      config_expand,
      &row,
      Some(expand),
      file_metadata_sanitizer,
    )
    .map_err(|err| RecordError::Internal(err.into()))?
  } else {
    record_to_json_expand_ref(
      api.columns(),
      config_expand,
      &row,
      None,
      file_metadata_sanitizer,
    )
    .map_err(|err| RecordError::Internal(err.into()))?
  };

  #[cfg(test)]
  test_only_validate_api_json_schema(
    &state,
    &api,
    trailbase_schema::json_schema::JsonSchemaMode::Select,
    // Expensive clone.
    &trailbase_schema::json::Value::Object(json_obj.clone()).into(),
  )?;

  return Ok(RawJson(
    serde_json::value::to_raw_value(&trailbase_schema::json::Value::Object(json_obj))
      .map_err(|err| RecordError::Internal(err.into()))?,
  ));
}

type GetUploadedFileFromRecordPath = Path<(
  String, // RecordApi name
  String, // Record id
  String, // Column name,
)>;

/// Read file associated with record.
///
/// You may prefer using the more general "files" (plural) handler below. Since using unique
/// filenames does help with the content life-cycle, such as caching.
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

  let Some(column_metadata) = api.column_metadata_by_name(&column_name) else {
    return Err(RecordError::BadRequest("invalid column name"));
  };

  let record_id = api.primary_key_to_value(record)?;
  let query =
    ReadFileRecordApiQuery::new_single_file_upload(api.clone(), column_metadata, record_id)?;
  query.check_table_level_access(user.as_ref())?;

  let json = api
    .conn()
    .call_reader(move |mut conn| {
      query.check_record_level_access(&mut conn, user.as_ref())?;
      return query.read(&mut conn);
    })
    .await?;

  let file_upload: FileUpload =
    serde_json::from_str(&json).map_err(|err| RecordError::Internal(err.into()))?;

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

  let Some(column_metadata) = api.column_metadata_by_name(&column_name) else {
    return Err(RecordError::BadRequest("invalid column name"));
  };

  let Some(JsonColumnMetadata::SchemaName(name)) = &column_metadata.json else {
    return Err(RecordError::BadRequest("not a file column"));
  };

  let record_id = api.primary_key_to_value(record)?;

  let file_uploads: Vec<FileUpload> = match name.as_str() {
    "std.FileUploads" => {
      let query =
        ReadFileRecordApiQuery::new_file_uploads(api.clone(), column_metadata, record_id)?;
      query.check_table_level_access(user.as_ref())?;

      let json = api
        .conn()
        .call_reader(move |mut conn| {
          query.check_record_level_access(&mut conn, user.as_ref())?;
          return query.read(&mut conn);
        })
        .await?;

      serde_json::from_str(&json).map_err(|err| RecordError::Internal(err.into()))?
    }
    "std.FileUpload" => {
      let query =
        ReadFileRecordApiQuery::new_single_file_upload(api.clone(), column_metadata, record_id)?;
      query.check_table_level_access(user.as_ref())?;

      let json = api
        .conn()
        .call_reader(move |mut conn| {
          query.check_record_level_access(&mut conn, user.as_ref())?;
          return query.read(&mut conn);
        })
        .await?;

      vec![serde_json::from_str(&json).map_err(|err| RecordError::Internal(err.into()))?]
    }
    _ => {
      return Err(RecordError::BadRequest("column mismatch"));
    }
  };

  let file_upload = file_uploads
    .into_iter()
    .find(|f| f.filename() == file_name)
    .ok_or_else(|| RecordError::RecordNotFound)?;

  return read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| RecordError::Internal(err.into()));
}

#[cfg(test)]
pub fn test_only_validate_api_json_schema(
  state: &AppState,
  api: &crate::records::RecordApi,
  mode: trailbase_schema::json_schema::JsonSchemaMode,
  value: &serde_json::Value,
) -> Result<(), RecordError> {
  let (validator, json_schema) =
    crate::records::json_schema::build_api_json_schema_internal(state, api, mode)?;

  let result = validator.evaluate(value);
  let errors: Vec<_> = result.iter_errors().collect();
  if errors.is_empty() {
    return Ok(());
  }

  return Err(RecordError::Internal(
    format!("Json value '{value}' does not match '{json_schema}: {errors:?}' ").into(),
  ));
}

#[cfg(test)]
mod tests {
  use axum::extract::{Path, Query, State};
  use base64::prelude::*;
  use object_store::ObjectStoreExt;
  use serde::Serialize;
  use serde_json::json;
  use trailbase_schema::QualifiedName;
  use trailbase_schema::db::QualifiedNameEscaped;
  use trailbase_schema::json_schema::{
    FileUpload, FileUploadData, FileUploadRequest, FileUploadResponse,
  };
  use trailbase_sqlite::ConnectionType;
  use uuid::Uuid;

  use super::*;
  use crate::app_state::*;
  use crate::auth::user::User;
  use crate::config::proto::{self, PermissionFlag, RecordApiConfig};
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::records::create_record::{
    CreateRecordQuery, CreateRecordResponse, create_record_handler,
  };
  use crate::records::delete_record::delete_record_handler;
  use crate::records::file_id::file_token_decrypt;
  use crate::records::files::FileDeletionsDb;
  use crate::records::params::JsonRow;
  use crate::records::test_utils::*;
  use crate::records::update_record::update_record_handler;
  use crate::test_util::unpack_json_response;
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
      .read_query_row_get(
        format!(r#"SELECT COUNT(*) from "{USER_TABLE}" WHERE email = :email"#),
        trailbase_sqlite::named_params! {
          ":email": EMAIL,
          ":unused": "unused",
          ":foo": 42,
        },
        0,
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

    // Register message table as record api with moderator read access.
    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("messages_api".to_string()),
        table_name: Some("message".to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        read_access_rule: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND \"user\" = _USER_.id))".to_string()),
        ..Default::default()
      },
    ).await.unwrap();

    let TestUserAndLogin {
      user: user_x,
      login: user_x_tokens,
    } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email("user_x@test.org".to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    add_user_to_room(conn, user_x.id, room0).await.unwrap();
    add_user_to_room(conn, user_x.id, room1).await.unwrap();

    let TestUserAndLogin {
      user: user_y,
      login: user_y_tokens,
    } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email("user_y@foo.baz".to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    add_user_to_room(conn, user_y.id, room0).await.unwrap();

    // Finally, create some messages and try to access them.
    {
      // Post to room0. X, Y, and mod should be able to read it.
      let message_id = send_message(conn, user_x.id, room0, "from user_x to room0")
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
          User::from_auth_token(&state, &user_x_tokens.auth_token),
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
          User::from_auth_token(&state, &user_y_tokens.auth_token),
        )
        .await;
        assert!(response.is_ok(), "{response:?}");
      }
    }

    {
      // Post to room1. Only X, and mod should be able to read it. User Y is not a member
      let message_id = send_message(conn, user_x.id, room1, "from user_x to room1")
        .await
        .unwrap();

      // User Y
      let response = read_record_handler(
        State(state.clone()),
        Path(("messages_api".to_string(), id_to_b64(&message_id))),
        Query(ReadRecordQuery::default()),
        User::from_auth_token(&state, &user_y_tokens.auth_token),
      )
      .await;
      assert!(response.is_err(), "{response:?}");
    }
  }

  async fn create_test_record_api(
    state: &AppState,
    api_name: &str,
    table_name: Option<&str>,
    db_name: Option<&str>,
  ) {
    let table_name = table_name.unwrap_or("table 😍");
    let fq_table_name = if let Some(db_name) = db_name {
      format!("\"{db_name}\".\"{table_name}\"")
    } else {
      format!("\"{table_name}\"")
    };

    let conn = if let Some(db_name) = db_name {
      state
        .connection_manager()
        .build_for_admin_ui_query_editor(Some([db_name.to_string()].into()))
        .await
        .unwrap()
        .connection
    } else {
      state.conn().clone()
    };

    conn
      .execute(
        format!(
          r#"
            CREATE TABLE {fq_table_name} (
              id           {uuid} PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT(uuid_v7()),
              -- TODO: Should this be a JSON(B)?
              file         {json} CHECK(jsonschema('std.FileUpload', file)),
              files        {json} CHECK(jsonschema('std.FileUploads', files)),
              -- Add a "keyword" column to ensure escaping is correct.
              "index"      TEXT NOT NULL DEFAULT(''),
              -- A special char column to check more escaping.
              "test 😍"    TEXT NOT NULL DEFAULT('')
            ) {strict};
          "#,
          strict = strict(&conn),
          uuid = uuid_column(&conn),
          json = json_column(&conn),
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
        table_name: Some(fq_table_name),
        attached_databases: if let Some(db_name) = db_name {
          vec![db_name.to_string()]
        } else {
          vec![]
        },
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

  #[tokio::test]
  async fn test_empty_create_record() {
    const API_NAME: &str = "test_api";

    let tables = [
      QualifiedName {
        name: "table".to_string(),
        database_schema: None,
      },
      #[cfg(not(feature = "pg-test"))]
      QualifiedName {
        name: "table".to_string(),
        database_schema: Some("other".to_string()),
      },
    ];

    for QualifiedName {
      name,
      database_schema,
    } in tables
    {
      let state = test_state(Some(TestStateOptions {
        config: Some({
          let mut config = test_config();

          if let Some(ref db) = database_schema {
            config.databases.push(proto::DatabaseConfig {
              name: Some(db.to_string()),
            });
          }

          config
        }),
        ..Default::default()
      }))
      .await
      .unwrap();

      create_test_record_api(&state, API_NAME, Some(&name), database_schema.as_deref()).await;

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

      let RawJson(_) = read_record_handler(
        State(state),
        Path(record_path),
        Query(ReadRecordQuery::default()),
        None,
      )
      .await
      .unwrap();
    }
  }

  #[tokio::test]
  async fn test_escaping_keywords_for_create_record() {
    const API_NAME: &str = "table";
    let state = test_state(None).await.unwrap();
    create_test_record_api(&state, API_NAME, None, None).await;

    let column_value = "test";
    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(API_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        None,
        Either::Json(json!({
          "index": column_value.to_string(),
          "test 😍": column_value.to_string(),
        })),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    let record_path = (API_NAME.to_string(), create_response.ids[0].clone());

    let RawJson(map) = read_record_handler(
      State(state),
      Path(record_path),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let map = to_object(&map);

    assert_eq!(
      *map.get("index").unwrap(),
      serde_json::Value::String(column_value.to_string())
    );

    assert_eq!(
      *map.get("test 😍").unwrap(),
      serde_json::Value::String(column_value.to_string())
    );
  }

  #[tokio::test]
  async fn test_single_file_upload_download_e2e() {
    let state = test_state(None).await.unwrap();
    const API_NAME: &str = "test_api";
    create_test_record_api(&state, API_NAME, None, None).await;

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
            file_column: FileUploadData {
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

    let RawJson(obj) = read_record_handler(
      State(state.clone()),
      Path(record_path.clone()),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let obj = to_object(&obj);

    let file = obj.get("file").unwrap();
    if let serde_json::Value::Object(obj) = file {
      // Test that we strip out the internal objectstore id. Update this test, if we'd ever add an
      // (maybe encrypted) id that can be safely round-tripped.
      assert!(!obj.contains_key("id"));
    } else {
      panic!("not an obj");
    }

    let file_upload: FileUploadResponse =
      serde_json::from_value(obj.get("file").unwrap().clone()).unwrap();
    assert_eq!(
      Some("bar"),
      file_upload.original_filename.as_deref(),
      "{file_upload:?}"
    );
    assert_eq!(Some("baz"), file_upload.content_type.as_deref());

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

    // Yield to give deletions a change to run async.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // We're checking that the uploads directory is empty.
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

  #[tokio::test]
  async fn test_file_upload_read_and_update() {
    let state = test_state(None).await.unwrap();

    const TABLE_NAME: &str = "test_table";
    const API_NAME: &str = "test_api";
    const FILE_COLUMN_NAME: &str = "file";
    create_test_record_api(&state, API_NAME, Some(TABLE_NAME), None).await;

    let bytes: Vec<u8> = vec![0, 1, 2, 3, 4, 5];
    let request = json!({
      FILE_COLUMN_NAME : FileUploadData {
        name: Some("foo0".to_string()),
        filename: Some("bar0".to_string()),
        content_type: Some("baz0".to_string()),
        data: bytes.clone(),
      },
    });

    let create_resp: CreateRecordResponse = unpack_json_response(
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

    assert_eq!(1, create_resp.ids.len());
    let id = &create_resp.ids[0];

    let RawJson(read_resp) = read_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), id.clone())),
      Query(ReadRecordQuery { expand: None }),
      None,
    )
    .await
    .unwrap();

    let obj = to_object(&read_resp);
    let file_metadata: FileUploadResponse =
      serde_json::from_value(obj.get(FILE_COLUMN_NAME).unwrap().clone()).unwrap();

    let Some(encrypted_token) = file_metadata.token.as_ref() else {
      panic!("missing file token");
    };

    let decrypted_token = file_token_decrypt(
      state.file_token_key(),
      &QualifiedNameEscaped::new(&QualifiedName {
        name: TABLE_NAME.to_string(),
        database_schema: Some("main".to_string()),
      }),
      encrypted_token,
    )
    .unwrap();

    // Round-trip the file token and consequently re-read.
    let _ = update_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), id.clone())),
      None,
      Either::Json(
        json_row_from_value(json!({
            FILE_COLUMN_NAME: file_metadata,
            "test 😍": "sentinel",
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    let RawJson(reread_resp) = read_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), id.clone())),
      Query(ReadRecordQuery { expand: None }),
      None,
    )
    .await
    .unwrap();

    let obj = to_object(&reread_resp);

    assert_eq!(
      Some(&serde_json::Value::String("sentinel".into())),
      obj.get("test 😍")
    );

    let reread_file: FileUploadResponse =
      serde_json::from_value(obj.get(FILE_COLUMN_NAME).unwrap().clone()).unwrap();

    assert!(reread_file.token.is_some());
    let reread_decrypted_token = file_token_decrypt(
      state.file_token_key(),
      &QualifiedNameEscaped::new(&QualifiedName {
        name: TABLE_NAME.to_string(),
        database_schema: Some("main".to_string()),
      }),
      reread_file.token.as_deref().unwrap(),
    )
    .unwrap();

    assert_eq!(decrypted_token, reread_decrypted_token);

    let read_response = get_uploaded_file_from_record_handler(
      State(state.clone()),
      Path((
        API_NAME.to_string(),
        id.clone(),
        FILE_COLUMN_NAME.to_string(),
      )),
      None,
    )
    .await
    .unwrap();

    let body = axum::body::to_bytes(read_response.into_body(), usize::MAX)
      .await
      .unwrap();
    assert_eq!(body.to_vec(), bytes);
  }

  #[tokio::test]
  async fn test_multiple_file_upload_download_e2e_and_deletion() {
    let state = test_state(None).await.unwrap();

    const API_NAME: &str = "test_api";
    const TABLE_NAME: &str = "test_table";

    create_test_record_api(&state, API_NAME, Some(TABLE_NAME), None).await;

    let file0 = FileUploadData {
      name: Some("foo0".to_string()),
      filename: Some("bar0".to_string()),
      content_type: Some("baz0".to_string()),
      data: vec![0, 1, 2, 3, 4, 5],
    };

    let file1 = FileUploadData {
      name: Some("foo1".to_string()),
      filename: Some("bar1".to_string()),
      content_type: Some("baz1".to_string()),
      data: vec![0, 1, 1, 2],
    };

    let file2 = FileUploadData {
      name: Some("foo2".to_string()),
      filename: Some("bar2".to_string()),
      content_type: Some("baz2".to_string()),
      data: vec![42, 5, 42, 5],
    };

    #[derive(Serialize)]
    struct PartialRequestSchema {
      file: FileUploadRequest,
      files: Vec<FileUploadRequest>,
    }

    #[derive(Deserialize)]
    struct PartialResponseSchema {
      file: FileUploadResponse,
      files: Vec<FileUploadResponse>,
    }

    let request = serde_json::to_value(&PartialRequestSchema {
      file: FileUploadRequest::Data(file0.clone()),
      files: vec![
        FileUploadRequest::Data(file1.clone()),
        FileUploadRequest::Data(file2.clone()),
      ],
    })
    .unwrap();

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

    let conn = state.conn().clone();

    // Checks the contents of all 3 files for the given record id and returns the file id.
    let assert_all_files_contents = async |record_id: &str| -> Vec<Uuid> {
      let record_uuid = BASE64_URL_SAFE.decode(record_id).unwrap();

      // NOTE: We cannot use `read_record_handler` here because we're striping the object-store uuid
      // from the public response.
      let row = conn
        .read_query_row(
          format!("SELECT file, files FROM {TABLE_NAME} WHERE id = ?1"),
          (record_uuid,),
        )
        .await
        .unwrap()
        .unwrap();

      let file: FileUpload = serde_json::from_str(&row.get::<String>(0).unwrap()).unwrap();
      let files: Vec<FileUpload> = serde_json::from_str(&row.get::<String>(1).unwrap()).unwrap();

      // Helper;
      async fn assert_file(
        state: &AppState,
        index: i64,
        expected: &[u8],
        f: &FileUpload,
        read: impl AsyncFnOnce() -> Response,
      ) -> Uuid {
        // Check metadata:
        assert_eq!(f.original_filename(), Some(format!("bar{index}").as_str()));
        assert_eq!(f.content_type(), Some(format!("baz{index}").as_str()));

        let uuid = f.objectstore_id();
        assert_eq!(4, uuid.get_version_num(), "invalid file/object uuid");

        // Make sure it file exists in objectstore and on file-system.
        assert!(check_file_with_id_exists(&state, uuid).await);

        // Check file contents:
        let response = read().await;
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
          .await
          .unwrap();

        assert_eq!(*expected, body);

        return uuid.clone();
      }

      return vec![
        // First file, i.e. `file`.
        assert_file(&state, 0, &file0.data, &file, async || {
          return get_uploaded_file_from_record_handler(
            State(state.clone()),
            Path((
              API_NAME.to_string(),
              record_id.to_string(),
              "file".to_string(),
            )),
            None,
          )
          .await
          .unwrap();
        })
        .await,
        // Second file, i.e. `files[0]`.
        assert_file(&state, 1, &file1.data, &files[0], async || {
          return get_uploaded_files_from_record_handler(
            State(state.clone()),
            Path((
              API_NAME.to_string(),
              record_id.to_string(),
              "files".to_string(),
              files[0].filename().to_string(),
            )),
            None,
          )
          .await
          .unwrap();
        })
        .await,
        // Third file, i.e. `files[1]`.
        assert_file(&state, 2, &file2.data, &files[1], async || {
          return get_uploaded_files_from_record_handler(
            State(state.clone()),
            Path((
              API_NAME.to_string(),
              record_id.to_string(),
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

    let record0_file_ids = assert_all_files_contents(&resp0.ids[0]).await;

    let RawJson(read_resp0) = read_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), resp0.ids[0].clone())),
      Query(ReadRecordQuery { expand: None }),
      None,
    )
    .await
    .unwrap();

    let read_resp0: PartialResponseSchema = serde_json::from_str(read_resp0.get()).unwrap();
    assert!(read_resp0.file.token.is_some());
    assert!(read_resp0.files[0].token.is_some());
    assert!(read_resp0.files[1].token.is_some());

    // Bulk insert: Insert two more records to check bulk creation.
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

    let record1_file_ids = assert_all_files_contents(&resp1.ids[0]).await;
    let record2_file_ids = assert_all_files_contents(&resp1.ids[1]).await;

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
      Either::Json(
        json_row_from_value(
          serde_json::to_value(&PartialRequestSchema {
            // NOTE: We're overriding file1 with the same but fresh data, skipping file1 and
            // round-tripping file2's token.
            file: FileUploadRequest::Data(file1.clone()),
            files: vec![FileUploadRequest::Token {
              token: read_resp0.files[1].token.clone().unwrap(),
            }],
          })
          .unwrap(),
        )
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    // Yield to give deletions a change to run async.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Make sure the _file_deletions have been processed
    let pending_deletions = state
      .conn()
      .read_query_rows(
        format!(
          "SELECT {columns} FROM {table}",
          columns = FileDeletionsDb::COLUMNS,
          table = FileDeletionsDb::TABLE_NAME
        ),
        (),
      )
      .await
      .unwrap()
      .into_iter()
      .map(FileDeletionsDb::from_row)
      .collect::<Result<Vec<_>, _>>()
      .unwrap();

    // Ok, all pending deletions have been processed:
    assert_eq!(0, pending_deletions.len(), "{pending_deletions:?}");

    // Check that the deleted files are actually gone, both in the store and on the fs.
    assert!(!check_file_with_id_exists(&state, &record0_file_ids[0]).await);
    assert!(!check_file_with_id_exists(&state, &record0_file_ids[1]).await);

    assert!(!check_file_with_id_exists(&state, &record1_file_ids[0]).await);
    assert!(!check_file_with_id_exists(&state, &record1_file_ids[1]).await);
    assert!(!check_file_with_id_exists(&state, &record1_file_ids[2]).await);

    assert!(!check_file_with_id_exists(&state, &record2_file_ids[0]).await);
    assert!(!check_file_with_id_exists(&state, &record2_file_ids[1]).await);
    assert!(!check_file_with_id_exists(&state, &record2_file_ids[2]).await);

    // Check that file2 still exists after their token being round-tripped in update
    // above.
    assert!(check_file_with_id_exists(&state, &record0_file_ids[2]).await);
  }

  async fn check_file_with_id_exists(state: &AppState, objectstore_id: &uuid::Uuid) -> bool {
    let path = objectstore_id.to_string();
    let exists_in_store = match state
      .objectstore()
      .get(&object_store::path::Path::from(path.clone()))
      .await
    {
      Ok(_) => true,
      Err(object_store::Error::NotFound { .. }) => false,
      Err(err) => {
        panic!("unexpedted objectore error: {err}");
      }
    };

    let exists_on_fs = std::fs::exists(state.data_dir().uploads_path().join(&path)).unwrap();

    return exists_in_store && exists_on_fs;
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
        format!("CREATE VIEW \"{view_name}\" AS SELECT * FROM {table_name}"),
        (),
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    let room0 = add_room(conn, "room0").await.unwrap();
    let room1 = add_room(conn, "room1").await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("messages_api".to_string()),
        table_name: Some(view_name.to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        read_access_rule: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND \"user\" = _USER_.id))".to_string()),
        ..Default::default()
      },
    )
    .await.unwrap();

    let TestUserAndLogin {
      user: user_x,
      login: user_x_tokens,
    } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email("user_x@test.org".to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    add_user_to_room(conn, user_x.id, room0).await.unwrap();
    add_user_to_room(conn, user_x.id, room1).await.unwrap();

    // Post to room0. X, Y, and mod should be able to read it.
    let message_id = send_message(conn, user_x.id, room0, "from user_x to room0")
      .await
      .unwrap();

    // User X
    let response = read_record_handler(
      State(state.clone()),
      Path(("messages_api".to_string(), id_to_b64(&message_id))),
      Query(ReadRecordQuery::default()),
      User::from_auth_token(&state, &user_x_tokens.auth_token),
    )
    .await;
    assert!(response.is_ok(), "{response:?}");
  }

  #[tokio::test]
  async fn test_record_api_with_excluded_columns() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    const API_NAME: &str = "test_api";

    conn
      .execute(
        // NOTE: The PK isn't auto-incrementing on PG, that's fine.
        format!(
          r#"
            CREATE TABLE "table" (
              pid          INTEGER PRIMARY KEY,
              "drop"       TEXT NOT NULL,
              "index"      TEXT NOT NULL DEFAULT('')
            ) {strict}
          "#,
          strict = strict(conn)
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

    let RawJson(json) = read_record_handler(
      State(state.clone()),
      Path((API_NAME.to_string(), create_response.ids[0].clone())),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    let json = to_object(&json);

    assert_eq!(serde_json::Value::Object(json), value);

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

    let index: String = conn
      .read_query_row_get(r#"SELECT "index" from "table" WHERE pid = 2"#, (), 0)
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
          r#"
            CREATE TABLE "{TABLE_NAME}" (
               id           {serial} PRIMARY KEY NOT NULL,
               col0         TEXT NOT NULL DEFAULT(''),
               col1         TEXT NOT NULL DEFAULT('')
            ) {strict}
          "#,
          strict = strict(conn),
          serial = serial_column(conn)
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
        create_access_rule: Some(match conn.connection_type() {
          // NOTE: PG's `IN` operator doesn't support SQLite's `IN (subquery)` syntax.
          // NOTE: We could use the same PG query in SQLite, just leaving the branch here to
          // highlight the difference.
          ConnectionType::Pg => "('col0' NOT IN (SELECT * FROM _REQ_FIELDS_))".to_string(),
          ConnectionType::Sqlite => "('col0' NOT IN _REQ_FIELDS_)".to_string(),
        }),
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
    let conn = state.conn();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE parent (
            pk           INTEGER PRIMARY KEY NOT NULL,
            value        TEXT NOT NULL,
            other        TEXT NOT NULL
          ) {strict};
          INSERT INTO parent (pk, value, other) VALUES (1, 'first', '1st'), (2, 'second', '2nd');

          CREATE TABLE child (
            id           INTEGER PRIMARY KEY NOT NULL,
            parent       INTEGER REFERENCES parent NOT NULL
          ) {strict};
          INSERT INTO child (id, parent) VALUES (1, 1), (2, 2);

          CREATE VIEW child_view AS SELECT * FROM child;
       "#,
        strict = strict(conn),
      ))
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
          "pk": 1,
          "value":"first",
          "other":"1st",
        },
      },
    });

    let RawJson(obj) = read_record_handler(
      State(state.clone()),
      Path(("child_api".to_string(), "1".to_string())),
      Query(ReadRecordQuery {
        expand: Some("parent".to_string()),
      }),
      None,
    )
    .await
    .unwrap();

    let obj = to_object(&obj);

    assert_eq!(serde_json::Value::Object(obj), expected);

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

    let RawJson(obj) = read_record_handler(
      State(state.clone()),
      Path(("child_view_api".to_string(), "1".to_string())),
      Query(ReadRecordQuery {
        expand: Some("parent".to_string()),
      }),
      None,
    )
    .await
    .unwrap();

    let obj = to_object(&obj);

    assert_eq!(serde_json::Value::Object(obj), expected);
  }

  // NOTE: Fails config validation for a PG connection ("custom schemas not (yet) supported...").
  #[cfg(not(feature = "pg-test"))]
  #[tokio::test]
  async fn test_custom_schema() {
    #[derive(serde::Serialize, schemars::JsonSchema)]
    struct StringArray(Vec<String>);

    let config = {
      let mut config = test_config();

      config.schemas.push(crate::config::proto::JsonSchemaConfig {
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

    let conn = state.conn();
    conn
      .execute(
        format!(
          r#"
            CREATE TABLE "{name}" (
              id           {serial} PRIMARY KEY,
              list         {json} NOT NULL CHECK(jsonschema('StringArray', list)) DEFAULT '[]'
            ) {strict}
          "#,
          strict = strict(conn),
          serial = serial_column(conn),
          json = json_column(conn),
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

    let create_response = create_record_handler(
      State(state.clone()),
      Path(name.clone()),
      Query(CreateRecordQuery::default()),
      None,
      Either::Json(record.clone()),
    )
    .await;

    match conn.connection_type() {
      // PG doesn't (yet) support custom schemas as defined for column `list` above.
      ConnectionType::Pg => {
        assert!(create_response.is_err());
      }
      ConnectionType::Sqlite => {
        let create_response: CreateRecordResponse = unpack_json_response(create_response.unwrap())
          .await
          .unwrap();

        let RawJson(obj) = read_record_handler(
          State(state),
          Path((name.clone(), create_response.ids[0].clone())),
          Query(ReadRecordQuery::default()),
          None,
        )
        .await
        .unwrap();

        let obj = to_object(&obj);

        assert_eq!(serde_json::Value::Object(obj), record);
      }
    }
  }

  // TODO: enable PostGIS with oliphaunt-wasix.
  #[cfg(all(
    any(feature = "geos", feature = "geos-static"),
    not(feature = "pg-test")
  ))]
  #[tokio::test]
  async fn test_geometry_columns_and_geojson() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    let name = "with_geo".to_string();
    conn
      .execute(
        format!(
          r#"
          CREATE TABLE "{name}" (
            id           INTEGER PRIMARY KEY,
            geo          {blob} CHECK(ST_IsValid(geo))
          ) {strict}
          "#,
          strict = strict(conn),
          blob = blob_column(conn),
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

    let json_geometry: geos::geojson::Geometry = {
      let coords = geos::CoordSeq::new_from_vec(&[&[12.4924, 41.8902]]).unwrap();
      let geometry = geos::Geometry::create_point(coords).unwrap();
      geometry.try_into().unwrap()
    };

    let record = json!({
      "id": 1,
      "geo": json_geometry,
    });

    let geojson = record
      .as_object()
      .unwrap()
      .get("geo")
      .unwrap()
      .as_object()
      .unwrap();
    assert_eq!(
      geojson.get("type").unwrap().as_str().unwrap(),
      "Point",
      "{geojson:?}"
    );

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

    let RawJson(obj) = read_record_handler(
      State(state),
      Path((name.clone(), create_response.ids[0].clone())),
      Query(ReadRecordQuery::default()),
      None,
    )
    .await
    .unwrap();

    assert_eq!(serde_json::Value::Object(to_object(&obj)), record);
  }
}
