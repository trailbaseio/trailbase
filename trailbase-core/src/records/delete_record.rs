use axum::{
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::records::query_builder::DeleteQueryBuilder;
use crate::records::{Permission, RecordError};

/// Delete record.
#[utoipa::path(
  delete,
  path = "/:name/:record",
  responses(
    (status = 200, description = "Successful deletion.")
  )
)]
pub async fn delete_record_handler(
  State(state): State<AppState>,
  Path((api_name, record)): Path<(String, String)>,
  user: Option<User>,
) -> Result<Response, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  let table_metadata = api
    .table_metadata()
    .ok_or_else(|| RecordError::ApiRequiresTable)?;

  let record_id = api.id_to_sql(&record)?;

  api
    .check_record_level_access(Permission::Delete, Some(&record_id), None, user.as_ref())
    .await?;

  DeleteQueryBuilder::run(
    &state,
    table_metadata,
    &api.record_pk_column().name,
    record_id,
  )
  .await
  .map_err(|err| RecordError::Internal(err.into()))?;

  return Ok((StatusCode::OK, "deleted").into_response());
}

#[cfg(test)]
mod test {
  use axum::extract::Query;
  use trailbase_sqlite::params;

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::User;
  use crate::config::proto::PermissionFlag;
  use crate::extract::Either;
  use crate::records::create_record::{
    create_record_handler, CreateRecordQuery, CreateRecordResponse,
  };
  use crate::records::test_utils::*;
  use crate::records::*;
  use crate::test::unpack_json_response;
  use crate::util::{b64_to_id, id_to_b64};

  #[tokio::test]
  async fn test_record_api_delete() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room = add_room(conn, "room0").await.unwrap();
    let password = "Secret!1!!";

    // Register message table as api with moderator read access.
    add_record_api(
      &state,
      "messages_api",
      "message",
      Acls {
        authenticated: vec![
          PermissionFlag::Create,
          PermissionFlag::Read,
          PermissionFlag::Delete,
        ],
        ..Default::default()
      },
      AccessRules {
        create: Some(
          "EXISTS(SELECT 1 FROM room_members WHERE room = _REQ_.room AND user = _USER_.id)"
            .to_string(),
        ),
        // Only owners can delete.
        delete: Some("(_ROW_._owner = _USER_.id)".to_string()),
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

    {
      // User X can delete their own message.
      let id = add_message(&state, &user_x, &user_x_token.auth_token, &room)
        .await
        .unwrap();
      delete_message(&state, &user_x_token.auth_token, &id)
        .await
        .unwrap();
      assert_eq!(message_exists(conn, &id).await, false);
    }

    {
      // User Y cannot delete X's message.
      let id = add_message(&state, &user_x, &user_x_token.auth_token, &room)
        .await
        .unwrap();
      let response = delete_message(&state, &user_y_token.auth_token, &id).await;
      assert!(response.is_err());
      assert_eq!(message_exists(conn, &id).await, true);
    }
  }

  async fn message_exists(conn: &trailbase_sqlite::Connection, id: &[u8; 16]) -> bool {
    let count: i64 = conn
      .query_value("SELECT COUNT(*) FROM message WHERE mid = $1", params!(*id))
      .await
      .unwrap()
      .unwrap();

    return count > 0;
  }

  async fn add_message(
    state: &AppState,
    user: &[u8; 16],
    auth_token: &str,
    room: &[u8; 16],
  ) -> Result<[u8; 16], anyhow::Error> {
    let create_json = serde_json::json!({
      "_owner": id_to_b64(&user),
      "room": id_to_b64(&room),
      "data": "user_x message to room",
    });

    let create_response = create_record_handler(
      State(state.clone()),
      Path("messages_api".to_string()),
      Query(CreateRecordQuery::default()),
      User::from_auth_token(state, auth_token),
      Either::Json(json_row_from_value(create_json).unwrap().into()),
    )
    .await;

    assert!(create_response.is_ok(), "{create_response:?}");

    let response: CreateRecordResponse = unpack_json_response(create_response.unwrap())
      .await
      .unwrap();

    return Ok(b64_to_id(&response.ids[0])?);
  }

  async fn delete_message(
    state: &AppState,
    auth_token: &str,
    id: &[u8; 16],
  ) -> Result<(), anyhow::Error> {
    delete_record_handler(
      State(state.clone()),
      Path(("messages_api".to_string(), id_to_b64(&id))),
      User::from_auth_token(state, auth_token),
    )
    .await?;
    return Ok(());
  }
}
