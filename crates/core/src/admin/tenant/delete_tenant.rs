use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Deserialize;
use trailbase_sqlite::params;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::tenant::DbTenant;
use crate::constants::TENANT_TABLE;

#[derive(Debug, Deserialize, Default, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DeleteTenantRequest {
  pub id: uuid::Uuid,
  pub remove_database: Option<bool>,
}

#[utoipa::path(
  delete,
  path = "/tenant",
  tag = "admin",
  request_body = DeleteTenantRequest,
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn delete_tenant_handler(
  State(state): State<AppState>,
  Json(request): Json<DeleteTenantRequest>,
) -> Result<Response, Error> {
  let deleted_tenant = DbTenant::from_row(
    state
      .user_conn()
      .write_query_row(
        format!(
          "DELETE FROM \"{TENANT_TABLE}\" WHERE id = $1 RETURNING {columns}",
          columns = DbTenant::COLUMNS
        ),
        params!(request.id.into_bytes()),
      )
      .await?
      .ok_or_else(|| Error::Precondition("not found".into()))?,
  )?;

  if request.remove_database == Some(true) {
    std::fs::remove_file(
      state
        .data_dir()
        .tenant_db_dir(&deleted_tenant.template)
        .join(format!("{}.db", request.id)),
    )
    .map_err(|err| Error::Internal(format!("delete db: {err}").into()))?;
  }

  return Ok((StatusCode::OK, "deleted").into_response());
}
