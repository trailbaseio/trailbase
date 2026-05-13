use axum::extract::{Path, State};
use axum::response::Response;
use const_format::formatcp;
use std::sync::LazyLock;
use trailbase_schema::metadata::{ColumnMetadata, TableMetadata};
use trailbase_schema::sqlite::{Column, ColumnOption, Table};
use trailbase_schema::{FileUploadInput, QualifiedName};

use crate::app_state::AppState;
use crate::auth::{AuthError, User};
use crate::config::proto::ConflictResolutionStrategy;
use crate::connection::ConnectionEntry;
use crate::constants::AVATAR_TABLE;
use crate::extract::Either;
use crate::records::RecordError;
use crate::records::params::{JsonRow, LazyParams};
use crate::records::read_queries::run_get_file_query;
use crate::records::write_queries::run_insert_query;
use crate::util::uuid_to_b64;

#[utoipa::path(
  get,
  path = "/avatar/:b64_user_id",
  tag = "auth",
  responses((status = 200, description = "Optional Avatar file"))
)]
pub async fn get_avatar_handler(
  State(state): State<AppState>,
  Path(b64_user_id): Path<String>,
) -> Result<Response, AuthError> {
  let Ok(user_id) = crate::util::b64_to_uuid(&b64_user_id) else {
    return Err(AuthError::BadRequest("Invalid user id"));
  };

  let conn = state.user_conn();
  let file_upload = run_get_file_query(
    conn,
    &trailbase_schema::QualifiedNameEscaped::new(&AVATAR_TABLE_NAME),
    &AVATAR_TABLE_FILE_COLUMN,
    "user",
    trailbase_sqlite::Value::Blob(user_id.into()),
  )
  .await
  .map_err(|err| match err {
    RecordError::RecordNotFound => AuthError::NotFound,
    _ => AuthError::Internal(err.into()),
  })?;

  return crate::records::files::read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| AuthError::Internal(err.into()));
}

#[utoipa::path(
  post,
  path = "/avatar/",
  tag = "auth",
  responses((status = 200, description = "Deletion success"))
)]
pub async fn create_avatar_handler(
  State(state): State<AppState>,
  user: User,
  either_request: Either<serde_json::Value>,
) -> Result<(), AuthError> {
  let conn = state.user_conn();

  #[cfg(all(not(feature = "pg"), debug_assertions))]
  {
    let ConnectionEntry { metadata, .. } = state.connection_manager().main_entry();
    let Some(table_metadata) = metadata.get_table(&AVATAR_TABLE_NAME) else {
      return Err(AuthError::Internal("missing table".into()));
    };

    assert_eq!(table_metadata.schema, AVATAR_TABLE_METADATA.schema);
  }

  let files: Vec<FileUploadInput> = match either_request {
    Either::Multipart(_value, files) => files,
    _ => {
      return Err(AuthError::BadRequest("expected multipart"));
    }
  };

  if files.len() != 1 || files[0].name.as_deref() != Some("file") {
    return Err(AuthError::BadRequest("Expected single 'file'"));
  }

  let record = JsonRow::from_iter([(
    "user".to_string(),
    serde_json::Value::String(uuid_to_b64(&user.uuid)),
  )]);

  let lazy_params = LazyParams::for_insert(
    &*AVATAR_TABLE_METADATA,
    state.json_schema_registry().clone(),
    record,
    Some(files),
  );
  let params = lazy_params
    .consume()
    .map_err(|_| AuthError::BadRequest("parameter conversion"))?;

  let _user_id_value = run_insert_query(
    conn,
    state.objectstore(),
    &trailbase_schema::QualifiedNameEscaped::new(&AVATAR_TABLE_NAME),
    Some(ConflictResolutionStrategy::Replace),
    "user",
    params,
  )
  .await
  .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(());
}

#[utoipa::path(
  delete,
  path = "/avatar/",
  tag = "auth",
  responses((status = 200, description = "Deletion success"))
)]
pub async fn delete_avatar_handler(
  State(state): State<AppState>,
  user: User,
) -> Result<(), AuthError> {
  const QUERY: &str = formatcp!("DELETE FROM '{AVATAR_TABLE}' WHERE user = ?1");

  let main_conn = state.connection_manager().main_entry().connection;
  main_conn
    .execute(QUERY, [trailbase_sqlite::Value::Blob(user.uuid.into())])
    .await?;

  return Ok(());
}

static AVATAR_TABLE_FILE_COLUMN: LazyLock<ColumnMetadata> = LazyLock::new(|| ColumnMetadata {
  index: 1,
  column: Column {
    name: String::from("file"),
    type_name: String::from("TEXT"),
    data_type: trailbase_schema::sqlite::ColumnDataType::Text,
    affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Text,
    options: vec![
      ColumnOption::Check(
        "jsonschema ('std.FileUpload', file, 'image/png, image/jpeg')".to_string(),
      ),
      ColumnOption::NotNull,
    ],
  },
  json: Some(trailbase_schema::metadata::JsonColumnMetadata::SchemaName(
    String::from("std.FileUpload"),
  )),
  is_file: true,
  is_geometry: false,
});

static AVATAR_TABLE_NAME: LazyLock<QualifiedName> = LazyLock::new(|| QualifiedName {
  name: AVATAR_TABLE.to_string(),
  database_schema: Some(if cfg!(feature = "pg") {
    "main".to_string()
  } else {
    "public".to_string()
  }),
});

// NOTE: We need TableMetadata to re-use the more generic RecordApi utilities for reading and
// writing file columns. We could get this from the schema registry, however the avatar table
// schema is always the same(as opposed to various RecordApi tables), so we may as well
// make it static. Moreover, this helps with the PG work in the interim.
static AVATAR_TABLE_METADATA: LazyLock<TableMetadata> = LazyLock::new(|| {
  let schema = Table {
    name: AVATAR_TABLE_NAME.clone(),
    strict: true,
    columns: vec![
      Column {
        name: String::from("user"),
        type_name: String::from("BLOB"),
        data_type: trailbase_schema::sqlite::ColumnDataType::Blob,
        affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Blob,
        options: vec![
          ColumnOption::Unique {
            is_primary: true,
            conflict_clause: None,
          },
          ColumnOption::NotNull,
          ColumnOption::ForeignKey {
            foreign_table: "_user".to_string(),
            referred_columns: vec!["id".to_string()],
            on_delete: Some(trailbase_schema::sqlite::ReferentialAction::Cascade),
            on_update: None,
          },
        ],
      },
      AVATAR_TABLE_FILE_COLUMN.column.clone(),
      Column {
        name: String::from("updated"),
        type_name: String::from("INTEGER"),
        data_type: trailbase_schema::sqlite::ColumnDataType::Integer,
        affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Integer,
        options: vec![
          ColumnOption::Default("(UNIXEPOCH ())".to_string()),
          ColumnOption::NotNull,
        ],
      },
    ],
    foreign_keys: vec![],
    unique: vec![],
    checks: vec![],
    virtual_table: false,
    temporary: false,
  };

  let json_schema_registry =
    trailbase_schema::registry::build_json_schema_registry(vec![]).expect("static");

  return TableMetadata::new(&json_schema_registry, schema.clone(), &[schema]).expect("static");
});

#[cfg(test)]
mod tests {
  use axum::extract::{FromRequest, Path, State};
  use axum::http;
  use axum::response::Response;
  use axum_test::multipart::{MultipartForm, Part};

  use super::*;
  use crate::admin::user::create_user_for_test;
  use crate::app_state::*;
  use crate::auth::user::{DbUser, User};
  use crate::auth::util::login_with_password;
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::util::id_to_b64;

  type Request = http::Request<axum::body::Body>;

  const COL_NAME: &str = "file";

  async fn build_upload_avatar_form_req(filename: &str, body_slice: &[u8]) -> Request {
    let form = MultipartForm::new().add_part(
      COL_NAME,
      Part::bytes(body_slice.to_vec()).file_name(filename),
    );
    let content_type = form.content_type();

    let body: axum::body::Body = form.into();

    http::Request::builder()
      .header("content-type", content_type)
      .body(body)
      .unwrap()
  }

  async fn upload_avatar(
    state: &AppState,
    user: User,
    body: &[u8],
  ) -> Result<uuid::Uuid, anyhow::Error> {
    let user_id = user.uuid;

    create_avatar_handler(
      State(state.clone()),
      user,
      Either::from_request(build_upload_avatar_form_req("foo.html", body).await, &())
        .await
        .unwrap(),
    )
    .await?;

    return Ok(user_id);
  }

  async fn download_avatar(state: &AppState, record_id: &[u8; 16]) -> Response {
    return get_avatar_handler(State(state.clone()), Path(id_to_b64(record_id)))
      .await
      .unwrap();
  }

  #[tokio::test]
  async fn test_avatar_upload() {
    let state = test_state(None).await.unwrap();

    let email = "user_x@test.com";
    let password = "SuperSecret5";
    let _user_x = create_user_for_test(&state, email, &password)
      .await
      .unwrap();

    let user_x_token = login_with_password(&state, email, password).await.unwrap();

    const QUERY: &str = formatcp!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#);

    let db_user = state
      .user_conn()
      .read_query_value::<DbUser>(QUERY, (email,))
      .await
      .unwrap()
      .unwrap();

    let missing_profile_response =
      get_avatar_handler(State(state.clone()), Path(id_to_b64(&db_user.id)))
        .await
        .err();
    assert!(
      matches!(missing_profile_response, Some(AuthError::NotFound)),
      "{missing_profile_response:?}"
    );

    const PNG0: &[u8] = b"\x89PNG\x0d\x0a\x1a\x0b";
    const PNG1: &[u8] = b"\x89PNG\x0d\x0a\x1a\x0c";

    let user = User::from_auth_token(&state, &user_x_token.auth_token).unwrap();
    let user_id = upload_avatar(&state, user.clone(), PNG0).await.unwrap();

    let response = download_avatar(&state, &user_id.into_bytes()).await;
    assert_eq!(
      axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap(),
      PNG0
    );

    // Test replacement
    let user_id = upload_avatar(&state, user.clone(), PNG1).await.unwrap();
    let response = download_avatar(&state, &user_id.into_bytes()).await;
    assert_eq!(
      axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap(),
      PNG1
    );

    // Test non png/jpeg types will be rejected
    assert!(
      upload_avatar(
        &state,
        User::from_auth_token(&state, &user_x_token.auth_token).unwrap(),
        b"<html><body>Body 0</body></html>",
      )
      .await
      .is_err()
    );

    let response = get_avatar_handler(State(state.clone()), Path(id_to_b64(&db_user.id)))
      .await
      .unwrap();
    assert_eq!(
      axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap(),
      PNG1
    );
  }
}
