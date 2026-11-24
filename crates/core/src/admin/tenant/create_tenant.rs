use axum::{Json, extract::State};
use const_format::formatcp;
use serde::{Deserialize, Serialize};
use trailbase_sqlite::named_params;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::tenant::DbTenant;
use crate::constants::TENANT_TABLE;

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
  let templates = state.access_config(|c| c.tenant_templates.clone());

  if templates
    .iter()
    .find(|t| t.name.as_ref() == Some(&request.template))
    .is_none()
  {
    return Err(Error::Precondition(format!(
      "template not found: {request:?}"
    )));
  }

  const INSERT_TENANT_QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{TENANT_TABLE}\" \
        (template, name) \
      VALUES \
        (:template, :name) \
      RETURNING {columns} \
    ",
    columns = DbTenant::COLUMNS,
  );

  let Some(tenant) = state
    .user_conn()
    .write_query_row(
      INSERT_TENANT_QUERY,
      named_params! {
        ":name": request.name,
        ":template": request.template,
      },
    )
    .await?
    .map(DbTenant::from_row)
    .transpose()?
  else {
    return Err(Error::Internal("Insert returned no tenant".into()));
  };

  return Ok(Json(CreateTenantResponse {
    id: Uuid::from_bytes(tenant.id),
  }));
}
