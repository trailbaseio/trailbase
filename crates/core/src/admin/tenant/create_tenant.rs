use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::tenant::create_new_tenant;

#[derive(Debug, Serialize, Deserialize, Default, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct CreateTenantRequest {
  pub template: String,
  pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Default, utoipa::ToSchema)]
pub struct CreateTenantResponse {
  pub id: Uuid,
}

#[utoipa::path(
  post,
  path = "/tenant",
  tag = "admin",
  request_body = CreateTenantRequest,
  responses(
    (status = 200, description = "Success", body = CreateTenantResponse),
  )
)]
pub async fn create_tenant_handler(
  State(state): State<AppState>,
  Json(request): Json<CreateTenantRequest>,
) -> Result<Json<CreateTenantResponse>, Error> {
  return Ok(Json(CreateTenantResponse {
    id: create_new_tenant(&state, request.template, request.name).await?,
  }));
}
