use axum::extract::{Json, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use serde::{Deserialize, Serialize};
use trailbase_sqlite::params;
use trailbase_sqlite::schema::FileUpload;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::user::DbUser;
use crate::auth::util::user_by_id;
use crate::auth::AuthError;
use crate::constants::{AVATAR_TABLE, RECORD_API_PATH};
use crate::util::{assert_uuidv7_version, id_to_b64};

async fn get_avatar_url(state: &AppState, user: &DbUser) -> Option<String> {
  if let Ok(row) = crate::util::query_one_row(
    state.user_conn(),
    &format!("SELECT EXISTS(SELECT user FROM '{AVATAR_TABLE}' WHERE user = $1)"),
    params!(user.id),
  )
  .await
  {
    let has_avatar: bool = row.get(0).unwrap_or(false);
    if has_avatar {
      let site = state.site_url();
      let record_user_id = id_to_b64(&user.id);
      let col_name = "file";
      return Some(format!(
        "{site}/{RECORD_API_PATH}/{AVATAR_TABLE}/{record_user_id}/file/{col_name}"
      ));
    }
  }

  return None;
}

/// Get a user's avatar url if available.
#[utoipa::path(
  get,
  path = "/avatar/:b64_user_id",
  responses((status = 200, description = "Optional Avatar url"))
)]
pub async fn get_avatar_url_handler(
  State(state): State<AppState>,
  headers: HeaderMap,
  Path(b64_user_id): Path<String>,
) -> Result<Response, AuthError> {
  let Ok(user_id) = crate::util::b64_to_uuid(&b64_user_id) else {
    return Err(AuthError::BadRequest("Invalid user id"));
  };
  assert_uuidv7_version(&user_id);

  let json = headers
    .get(header::CONTENT_TYPE)
    .map_or(false, |t| t == "application/json");

  let db_user = user_by_id(&state, &user_id).await?;

  // TODO: Allow a configurable fallback url.
  let avatar_url = get_avatar_url(&state, &db_user)
    .await
    .or(db_user.provider_avatar_url);

  // TODO: Maybe return a JSON response with url if content-type is JSON.
  return match avatar_url {
    Some(url) => {
      if json {
        Ok(
          Json(serde_json::json!({
            "avatar_url": url,
          }))
          .into_response(),
        )
      } else {
        Ok(Redirect::to(&url).into_response())
      }
    }
    None => Ok(StatusCode::NOT_FOUND.into_response()),
  };
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct DbAvatar {
  pub user: [u8; 16],
  pub file: String,
  pub updated: i64,
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Avatar {
  pub user: Uuid,
  pub file: FileUpload,
}

#[cfg(test)]
mod tests {
  use axum::extract::{FromRequest, Path, Query, State};
  use axum::http;
  use axum::response::Response;
  use axum_test::multipart::{MultipartForm, Part};

  use super::*;
  use crate::admin::user::create_user_for_test;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::{DbUser, User};
  use crate::constants::RECORD_API_PATH;
  use crate::constants::{AVATAR_TABLE, USER_TABLE};
  use crate::extract::Either;
  use crate::records::create_record::{
    create_record_handler, CreateRecordQuery, CreateRecordResponse,
  };
  use crate::records::read_record::get_uploaded_file_from_record_handler;
  use crate::test::unpack_json_response;
  use crate::util::{b64_to_uuid, id_to_b64, uuid_to_b64};

  type Request = http::Request<axum::body::Body>;

  const COL_NAME: &str = "file";
  const AVATAR_COLLECTION_NAME: &str = AVATAR_TABLE;

  async fn build_upload_avatar_form_req(
    user: &uuid::Uuid,
    filename: &str,
    body_slice: &[u8],
  ) -> Request {
    let user_id = uuid_to_b64(&user);

    let form = MultipartForm::new().add_text("user", user_id).add_part(
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
    user: Option<User>,
    body: &[u8],
  ) -> Result<uuid::Uuid, anyhow::Error> {
    let user_id = user.as_ref().unwrap().uuid;
    let response: CreateRecordResponse = unpack_json_response(
      create_record_handler(
        State(state.clone()),
        Path(AVATAR_COLLECTION_NAME.to_string()),
        Query(CreateRecordQuery::default()),
        user,
        Either::from_request(
          build_upload_avatar_form_req(&user_id, "foo.html", body).await,
          &(),
        )
        .await
        .unwrap(),
      )
      .await?,
    )
    .await
    .unwrap();

    return Ok(b64_to_uuid(&response.id)?);
  }

  async fn download_avatar(state: &AppState, record_id: &[u8; 16]) -> Response {
    return get_uploaded_file_from_record_handler(
      State(state.clone()),
      Path((
        AVATAR_COLLECTION_NAME.to_string(),
        id_to_b64(record_id),
        COL_NAME.to_string(),
      )),
      None,
    )
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

    let db_user = state
      .user_conn()
      .query_value::<DbUser>(
        &format!("SELECT * FROM '{USER_TABLE}' WHERE email = $1"),
        (email,),
      )
      .await
      .unwrap()
      .unwrap();

    let missing_profile_response = get_avatar_url_handler(
      State(state.clone()),
      HeaderMap::new(),
      Path(id_to_b64(&db_user.id)),
    )
    .await
    .unwrap();
    assert_eq!(
      missing_profile_response.status(),
      http::StatusCode::NOT_FOUND
    );

    const PNG0: &[u8] = b"\x89PNG\x0d\x0a\x1a\x0b";
    const PNG1: &[u8] = b"\x89PNG\x0d\x0a\x1a\x0c";

    let record_id = upload_avatar(
      &state,
      User::from_auth_token(&state, &user_x_token.auth_token),
      PNG0,
    )
    .await
    .unwrap();

    let response = download_avatar(&state, &record_id.into_bytes()).await;
    assert_eq!(
      axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap(),
      PNG0
    );

    // Test replacement
    let record_id = upload_avatar(
      &state,
      User::from_auth_token(&state, &user_x_token.auth_token),
      PNG1,
    )
    .await
    .unwrap();
    let response = download_avatar(&state, &record_id.into_bytes()).await;
    assert_eq!(
      axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap(),
      PNG1
    );

    // Test non png/jpeg types will be rejected
    assert!(upload_avatar(
      &state,
      User::from_auth_token(&state, &user_x_token.auth_token),
      b"<html><body>Body 0</body></html>",
    )
    .await
    .is_err());

    let avatar_response = get_avatar_url_handler(
      State(state.clone()),
      HeaderMap::new(),
      Path(id_to_b64(&db_user.id)),
    )
    .await
    .unwrap();

    assert_eq!(avatar_response.status(), http::StatusCode::SEE_OTHER);
    let location = avatar_response
      .headers()
      .get("location")
      .unwrap()
      .to_str()
      .unwrap();

    assert_eq!(
      location,
      format!(
        "{site}/{RECORD_API_PATH}/{AVATAR_COLLECTION_NAME}/{record_id_b64}/file/{COL_NAME}",
        site = state.site_url(),
        record_id_b64 = uuid_to_b64(&record_id),
      )
    );
  }
}
