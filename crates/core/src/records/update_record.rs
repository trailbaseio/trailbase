use axum::extract::{Path, State};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::extract::Either;
use crate::records::RecordError;
use crate::records::files::{AsyncFileManager, delete_files_marked_for_deletion};
use crate::records::params::{JsonRow, ParamOptions, UpdateParams};
use crate::records::queries::UpdateRecordApiQuery;

/// Update existing record.
#[utoipa::path(
  patch,
  path = "/{name}/{record}",
  tag = "records",
  request_body = serde_json::Value,
  responses(
    (status = 200, description = "Successful update.")
  )
)]
pub async fn update_record_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
  either_request: Either<JsonRow>,
) -> Result<(), RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  if !api.is_table() {
    return Err(RecordError::ApiRequiresTable);
  }

  let (request, multipart_files) = match either_request {
    Either::Json(value) => (value, None),
    Either::Multipart(value, files) => (value, Some(files)),
    Either::Form(value) => (value, None),
  };

  let record_id = api.primary_key_to_value(record)?;
  let conn = api.conn();

  let (params, files) = UpdateParams::new(
    &api,
    &state.json_schema_registry().load(),
    (&api.record_pk_column().column.name, record_id.clone()),
    request,
    ParamOptions {
      multipart_files,
      file_token_decoder: None,
    },
  )
  .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?;

  let (file_manager, file_committer) = if !files.is_empty() {
    let (file_manager, file_committer) = AsyncFileManager::new(state.objectstore().clone(), files);
    (Some(file_manager), Some(file_committer))
  } else {
    (None, None)
  };

  let query = UpdateRecordApiQuery::new(api.clone(), params)?;
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

  // Successful write, do not cleanup written files.
  if let Some(mut file_manager) = file_manager {
    file_manager.release().await;

    tokio::spawn(async move {
      if let Err(err) = delete_files_marked_for_deletion(
        api.conn(),
        state.objectstore(),
        api.table_name(),
        &[write_result.rowid],
      )
      .await
      {
        log::error!("Failed to delete files: {err}");
      }
    });
  }

  return Ok(());
}

#[cfg(test)]
mod tests {
  use axum::extract::Query;
  use serde_json::json;
  use trailbase_sqlite::params;

  use super::*;
  use crate::app_state::*;
  use crate::auth::user::User;
  use crate::config::proto::{self, PermissionFlag};
  use crate::constants::USER_TABLE;
  use crate::extract::Either;
  use crate::records::create_record::{
    CreateRecordQuery, CreateRecordResponse, create_record_handler,
  };
  use crate::records::test_utils::*;
  use crate::records::*;
  use crate::util::{b64_to_id, id_to_b64, uuid_to_b64};

  #[tokio::test]
  async fn test_simple_record_api_update() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE "update" (
            "id"      INTEGER PRIMARY KEY,
            "int"     INTEGER NOT NULL DEFAULT (-1),
            "float"   REAL NOT NULL,
            "text"    TEXT
          ) {strict};
        "#,
        strict = strict(conn)
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      proto::RecordApiConfig {
        name: Some("update_api".to_string()),
        table_name: Some("update".to_string()),
        acl_world: [
          PermissionFlag::Create as i32,
          PermissionFlag::Read as i32,
          PermissionFlag::Update as i32,
        ]
        .into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let _ = create_record_handler(
      State(state.clone()),
      Path("update_api".to_string()),
      Query(CreateRecordQuery::default()),
      None,
      Either::Json(
        json_row_from_value(json!({
          "id": 1,
          "float": 5,
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    let _ = update_record_handler(
      State(state.clone()),
      Path(("update_api".to_string(), "1".to_string())),
      None,
      Either::Json(
        json_row_from_value(json!({
          "id": 1,
          "int": 4,
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    assert_eq!(
      conn
        .read_query_row_get::<i64>(r#"SELECT "int" FROM "update" WHERE id = 1"#, (), 0)
        .await
        .unwrap(),
      Some(4)
    );

    // Test that bad input leads to bad request.
    let response = update_record_handler(
      State(state.clone()),
      Path(("update_api".to_string(), "1".to_string())),
      None,
      Either::Json(
        json_row_from_value(json!({
          "int": 4.1,
        }))
        .unwrap()
        .into(),
      ),
    )
    .await;

    assert!(
      matches!(response.as_ref().err().unwrap(), RecordError::BadRequest(_)),
      "{:?}",
      response.err()
    )
  }

  #[tokio::test]
  async fn test_record_api_update() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room = add_room(conn, "room0").await.unwrap();

    // Register message table and api with moderator read access.
    add_record_api(
      &state,
      "messages_api",
      "message",
      Acls {
        authenticated: vec![
          PermissionFlag::Create,
          PermissionFlag::Read,
          PermissionFlag::Update,
        ],
        ..Default::default()
      },
      AccessRules {
        create: Some(
          "EXISTS(SELECT 1 FROM room_members WHERE room = _REQ_.room AND \"user\" = _USER_.id)"
            .to_string(),
        ),
        update: Some(
          "EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND \"user\" = _USER_.id)"
            .to_string(),
        ),
        ..Default::default()
      },
    )
    .await
    .unwrap();

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

    add_user_to_room(conn, user_x.id, room).await.unwrap();

    let TestUserAndLogin {
      login: user_y_tokens,
      ..
    } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email("user_y@foo.baz".to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    let create_json = serde_json::json!({
      "_owner": uuid_to_b64(&user_x.id),
      "room": id_to_b64(&room),
      "data": "user_x message to room",
    });
    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_tokens.auth_token),
        Either::Json(json_row_from_value(create_json).unwrap().into()),
      )
      .await
      .unwrap(),
    )
    .await
    .unwrap();

    assert_eq!(create_response.ids.len(), 1);
    let b64_id = create_response.ids[0].clone();

    {
      // User X can modify their own message.
      let updated_message_text = "user_x updated message to room";
      let update_json = serde_json::json!({
        "data": updated_message_text,
      });
      let update_response = update_record_handler(
        State(state.clone()),
        Path(("messages_api".to_string(), b64_id.clone())),
        User::from_auth_token(&state, &user_x_tokens.auth_token),
        Either::Json(json_row_from_value(update_json).unwrap().into()),
      )
      .await;

      assert!(update_response.is_ok(), "{b64_id} {update_response:?}");

      let message_text: String = conn
        .read_query_row_get(
          "SELECT data FROM message WHERE mid = $1",
          params!(b64_to_id(&b64_id).unwrap()),
          0,
        )
        .await
        .unwrap()
        .unwrap();
      assert_eq!(updated_message_text, message_text);
    }

    {
      // User Y cannot modify User X's message.
      let update_json = serde_json::json!({
        "data": "invalid update by user y",
      });
      let update_response = update_record_handler(
        State(state.clone()),
        Path(("messages_api".to_string(), b64_id.clone())),
        User::from_auth_token(&state, &user_y_tokens.auth_token),
        Either::Json(json_row_from_value(update_json).unwrap().into()),
      )
      .await;

      assert!(update_response.is_err(), "{b64_id} {update_response:?}");
    }
  }

  #[tokio::test]
  async fn test_check_pk_column_cannot_be_overridden() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    let user_x_email = "user_x@test.org";
    let TestUserAndLogin {
      user: user_x,
      login: user_x_tokens,
    } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email(user_x_email.to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    conn
      .execute_batch(format!(
        r#"
          CREATE TABLE test (
            "user"    {uuid} PRIMARY KEY REFERENCES _user(id),
            "data"    TEXT NOT NULL
          ) {strict};

          INSERT INTO test ("user", data) SELECT id, 'secret' FROM {USER_TABLE} WHERE email = '{user_x_email}';
        "#,
        strict = strict(conn),
        uuid = uuid_column(conn),
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      proto::RecordApiConfig {
        name: Some("test_api".to_string()),
        table_name: Some("test".to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Update as i32].into(),
        update_access_rule: Some("_ROW_.user = _USER_.id".to_string()),
        create_access_rule: Some("_REQ_.user = _USER_.id".to_string()),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let _ = update_record_handler(
      State(state.clone()),
      Path(("test_api".to_string(), uuid_to_b64(&user_x.id))),
      User::from_auth_token(&state, &user_x_tokens.auth_token),
      Either::Json(
        json_row_from_value(json!({
            "user": uuid_to_b64(&user_x.id),
            "data": "updated secret",
        }))
        .unwrap()
        .into(),
      ),
    )
    .await
    .unwrap();

    // Ensure we cannot use update to sign a record over to another user
    let TestUserAndLogin { user: user_y, .. } = create_and_login_user_for_test(
      &state,
      UserIdentifier::Email("user_y@test.org".to_string()),
      "Secret!1!!",
    )
    .await
    .unwrap();

    assert!(
      update_record_handler(
        State(state.clone()),
        Path(("test_api".to_string(), uuid_to_b64(&user_x.id))),
        User::from_auth_token(&state, &user_x_tokens.auth_token),
        Either::Json(
          json_row_from_value(json!({
              "user": uuid_to_b64(&user_y.id),
              "data": "updated secret",
          }))
          .unwrap()
          .into(),
        ),
      )
      .await
      .is_err()
    );
  }
}
