use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tower_cookies::Cookies;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::auth::util::{delete_all_sessions_for_user, remove_all_cookies};
use crate::auth::AuthError;
use crate::constants::USER_TABLE;

/// Get public profile of the given user.
#[utoipa::path(
  delete,
  path = "/delete",
  responses(
    (status = 200, description = "User deleted.")
  )
)]
pub(crate) async fn delete_handler(
  State(state): State<AppState>,
  user: User,
  cookies: Cookies,
) -> Result<Response, AuthError> {
  let _ = delete_all_sessions_for_user(&state, user.uuid).await;

  state
    .user_conn()
    .execute(
      &format!("DELETE FROM '{USER_TABLE}' WHERE id = $1"),
      [tokio_rusqlite::Value::Blob(user.uuid.into_bytes().to_vec())],
    )
    .await?;

  remove_all_cookies(&cookies);

  return Ok((StatusCode::OK, "deleted").into_response());
}
