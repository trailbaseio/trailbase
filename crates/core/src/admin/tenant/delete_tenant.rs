use axum::{
  Json,
  extract::State,
  http::StatusCode,
  response::{IntoResponse, Response},
};
use serde::Deserialize;
use trailbase_schema::QualifiedName;
use trailbase_sqlvalue::{Blob, SqlValue};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::admin::rows::delete_rows::delete_row;
use crate::app_state::AppState;
use crate::constants::TENANT_TABLE;
use crate::util::uuid_to_b64;

#[derive(Debug, Deserialize, Default, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct DeleteTenantRequest {
  pub id: uuid::Uuid,
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
  delete_row(
    &state,
    &QualifiedName {
      name: TENANT_TABLE.to_string(),
      database_schema: None,
    },
    "id",
    SqlValue::Blob(Blob::Base64UrlSafe(uuid_to_b64(&request.id))),
  )
  .await?;

  return Ok((StatusCode::OK, "deleted").into_response());
}
