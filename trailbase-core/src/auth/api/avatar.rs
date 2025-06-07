use axum::extract::{Path, State};
use axum::response::Response;
use lazy_static::lazy_static;
use trailbase_schema::QualifiedName;

use crate::app_state::AppState;
use crate::auth::{AuthError, User};
use crate::config::proto::ConflictResolutionStrategy;
use crate::constants::AVATAR_TABLE;
use crate::extract::Either;
use crate::records::create_record::{RecordAndFiles, extract_record, extract_records};
use crate::records::params::LazyParams;
use crate::records::query_builder::QueryError;
use crate::util::{assert_uuidv7_version, uuid_to_b64};

#[utoipa::path(
  get,
  path = "/avatar/:b64_user_id",
  responses((status = 200, description = "Optional Avatar file"))
)]
pub async fn get_avatar_handler(
  State(state): State<AppState>,
  Path(b64_user_id): Path<String>,
) -> Result<Response, AuthError> {
  let Ok(user_id) = crate::util::b64_to_uuid(&b64_user_id) else {
    return Err(AuthError::BadRequest("Invalid user id"));
  };
  assert_uuidv7_version(&user_id);

  let Some(table) = state.schema_metadata().get_table(&table_name) else {
    return Err(AuthError::Internal("missing table".into()));
  };

  let Some((index, file_column)) = table.column_by_name("file") else {
    return Err(AuthError::Internal("missing column".into()));
  };

  let Some(ref column_json_metadata) = table.json_metadata.columns[index] else {
    return Err(AuthError::Internal("missing metadata".into()));
  };

  let file_upload = crate::records::query_builder::GetFileQueryBuilder::run(
    &state,
    &trailbase_schema::QualifiedNameEscaped::new(&table_name),
    file_column,
    column_json_metadata,
    "user",
    rusqlite::types::Value::Blob(user_id.into()),
  )
  .await
  .map_err(|err| match err {
    QueryError::NotFound => AuthError::NotFound,
    _ => AuthError::Internal(err.into()),
  })?;

  return crate::records::files::read_file_into_response(&state, file_upload)
    .await
    .map_err(|err| AuthError::Internal(err.into()));
}

#[utoipa::path(
  post,
  path = "/avatar/",
  responses((status = 200, description = "Deletion success"))
)]
pub async fn create_avatar_handler(
  State(state): State<AppState>,
  user: User,
  either_request: Either<serde_json::Value>,
) -> Result<(), AuthError> {
  let Some(table) = state.schema_metadata().get_table(&table_name) else {
    return Err(AuthError::Internal("missing table".into()));
  };

  let mut records_and_files: Vec<RecordAndFiles> = match either_request {
    Either::Json(value) => {
      extract_records(value).map_err(|_err| AuthError::BadRequest("extract json"))?
    }
    Either::Multipart(value, files) => vec![(
      extract_record(value).map_err(|_err| AuthError::BadRequest("extract multipart"))?,
      Some(files),
    )],
    Either::Form(value) => vec![(
      extract_record(value).map_err(|_err| AuthError::BadRequest("extract form"))?,
      None,
    )],
  };

  // TODO: Better input validation, i.e. ensure only one "file" provided. Consider only supporting
  // multipart form.
  if records_and_files.len() != 1 {
    return Err(AuthError::BadRequest("expected one file"));
  }

  let (mut record, files) = records_and_files.swap_remove(0);
  if record
    .insert(
      "user".to_string(),
      serde_json::Value::String(uuid_to_b64(&user.uuid)),
    )
    .is_some()
  {
    return Err(AuthError::BadRequest("pre-existing user"));
  }

  let lazy_params = LazyParams::new(&*table, record, files);
  let params = lazy_params
    .consume()
    .map_err(|_| AuthError::BadRequest("parameter conversion"))?;

  let _user_id_value = crate::records::query_builder::InsertQueryBuilder::run(
    &state,
    &trailbase_schema::QualifiedNameEscaped::new(&table_name),
    Some(ConflictResolutionStrategy::Replace),
    "user",
    true,
    params,
  )
  .await
  .map_err(|err| AuthError::Internal(err.into()))?;

  return Ok(());
}

#[utoipa::path(
  delete,
  path = "/avatar/",
  responses((status = 200, description = "Deletion success"))
)]
pub async fn delete_avatar_handler(
  State(state): State<AppState>,
  user: User,
) -> Result<(), AuthError> {
  lazy_static! {
    static ref SQL: String = format!("DELETE FROM {AVATAR_TABLE} WHERE user = ?1");
  }

  state
    .conn()
    .execute(&*SQL, [rusqlite::types::Value::Blob(user.uuid.into())])
    .await?;

  return Ok(());
}

lazy_static! {
  static ref table_name: QualifiedName = QualifiedName {
    name: AVATAR_TABLE.to_string(),
    database_schema: None,
  };
}

#[cfg(test)]
mod tests {
  use axum::extract::{FromRequest, Path, State};
  use axum::http;
  use axum::response::Response;
  use axum_test::multipart::{MultipartForm, Part};

  use super::*;
  use crate::admin::user::create_user_for_test;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::{DbUser, User};
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::util::{id_to_b64, uuid_to_b64};

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

    lazy_static! {
      static ref QUERY: String = format!(r#"SELECT * FROM "{USER_TABLE}" WHERE email = $1"#);
    };

    let db_user = state
      .user_conn()
      .read_query_value::<DbUser>(&*QUERY, (email,))
      .await
      .unwrap()
      .unwrap();

    let missing_profile_response =
      get_avatar_handler(State(state.clone()), Path(id_to_b64(&db_user.id)))
        .await
        .err();
    assert!(matches!(
      missing_profile_response,
      Some(AuthError::NotFound)
    ));

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
