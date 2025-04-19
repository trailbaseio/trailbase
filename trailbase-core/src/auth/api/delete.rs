use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use lazy_static::lazy_static;
use tower_cookies::Cookies;

use crate::app_state::AppState;
use crate::auth::AuthError;
use crate::auth::user::User;
use crate::auth::util::{delete_all_sessions_for_user, remove_all_cookies};
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

  lazy_static! {
    static ref QUERY: String = format!(r#"DELETE FROM "{USER_TABLE}" WHERE id = $1"#);
  }

  state
    .user_conn()
    .execute(&*QUERY, [trailbase_sqlite::Value::Blob(user.uuid.into())])
    .await?;

  remove_all_cookies(&cookies);

  return Ok((StatusCode::OK, "deleted").into_response());
}
