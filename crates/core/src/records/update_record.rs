use axum::extract::{Path, State};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::extract::Either;
use crate::records::params::{JsonRow, LazyParams};
use crate::records::query_builder::UpdateQueryBuilder;
use crate::records::{Permission, RecordError};

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
  let (_index, pk_column) = api.record_pk_column();

  let mut lazy_params = LazyParams::for_update(
    &api,
    request,
    multipart_files,
    pk_column.name.clone(),
    record_id.clone(),
  );

  api
    .check_record_level_access(
      Permission::Update,
      Some(&record_id),
      Some(&mut lazy_params),
      user.as_ref(),
    )
    .await?;

  UpdateQueryBuilder::run(
    &state,
    api.table_name(),
    api.has_file_columns(),
    lazy_params
      .consume()
      .map_err(|_err| RecordError::BadRequest("Invalid Parameters"))?,
  )
  .await
  .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok(());
}

#[cfg(test)]
mod test {
  use axum::extract::Query;
  use serde_json::json;
  use trailbase_sqlite::params;

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::user::User;
  use crate::auth::util::login_with_password;
  use crate::config::proto::PermissionFlag;
  use crate::extract::Either;
  use crate::records::create_record::{
    CreateRecordQuery, CreateRecordResponse, create_record_handler,
  };
  use crate::records::test_utils::*;
  use crate::records::*;
  use crate::test::unpack_json_response;
  use crate::util::{b64_to_id, id_to_b64};

  #[tokio::test]
  async fn test_simple_record_api_update() {
    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE "update" (
            "id"      INTEGER PRIMARY KEY,
            "int"     INTEGER NOT NULL DEFAULT (-1),
            "float"   REAL NOT NULL,
            "text"    TEXT
          ) STRICT;
        "#,
      )
      .await
      .unwrap();

    state.rebuild_schema_cache().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
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
      state
        .conn()
        .read_query_value::<i64>(r#"SELECT "int" FROM "update" WHERE id = 1"#, ())
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

    assert!(matches!(
      response.err().unwrap(),
      RecordError::BadRequest(_)
    ))
  }

  #[tokio::test]
  async fn test_record_api_update() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room = add_room(conn, "room0").await.unwrap();
    let password = "Secret!1!!";

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
          "EXISTS(SELECT 1 FROM room_members WHERE room = _REQ_.room AND user = _USER_.id)"
            .to_string(),
        ),
        update: Some(
          "EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND user = _USER_.id)"
            .to_string(),
        ),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let user_x_email = "user_x@test.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await
      .unwrap()
      .into_bytes();

    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    add_user_to_room(conn, user_x, room).await.unwrap();

    let user_y_email = "user_y@foo.baz";
    let _user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();

    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    let create_json = serde_json::json!({
      "_owner": id_to_b64(&user_x),
      "room": id_to_b64(&room),
      "data": "user_x message to room",
    });
    let create_response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path("messages_api".to_string()),
        Query(CreateRecordQuery::default()),
        User::from_auth_token(&state, &user_x_token.auth_token),
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
        User::from_auth_token(&state, &user_x_token.auth_token),
        Either::Json(json_row_from_value(update_json).unwrap().into()),
      )
      .await;

      assert!(update_response.is_ok(), "{b64_id} {update_response:?}");

      let message_text: String = conn
        .read_query_value(
          "SELECT data FROM message WHERE mid = $1",
          params!(b64_to_id(&b64_id).unwrap()),
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
        User::from_auth_token(&state, &user_y_token.auth_token),
        Either::Json(json_row_from_value(update_json).unwrap().into()),
      )
      .await;

      assert!(update_response.is_err(), "{b64_id} {update_response:?}");
    }
  }
}
