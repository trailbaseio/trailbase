use axum::{
  Json,
  extract::{RawQuery, State},
};
use const_format::formatcp;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::tenant::DbTenant;
use crate::connection::ConnectionEntry;
use crate::constants::TENANT_TABLE;
use crate::{admin::AdminError as Error, auth::AuthError};

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
pub struct TenantJson {
  pub id: Uuid,
  pub name: String,
  pub template: String,

  pub created: i64,
}

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ListTenantsResponse {
  total_row_count: usize,
  tenants: Vec<TenantJson>,
}

#[utoipa::path(
  get,
  path = "/tenant",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn list_tenants_handler(
  State(state): State<AppState>,
  // TODO: Add filter just like for list users.
  RawQuery(_raw_url_query): RawQuery,
) -> Result<Json<ListTenantsResponse>, Error> {
  let ConnectionEntry {
    connection: conn,
    metadata: _,
  } = state.connection_manager().main_entry();

  let tenants = conn
    .read_query_rows(formatcp!("SELECT * FROM {TENANT_TABLE}"), ())
    .await?
    .into_iter()
    .map(|row| -> Result<_, AuthError> {
      let DbTenant {
        id,
        name,
        template,
        created,
      } = DbTenant::from_row(row)?;
      return Ok(TenantJson {
        id: Uuid::from_bytes(id),
        name,
        template,
        created,
      });
    })
    .collect::<Result<Vec<_>, _>>()?;

  return Ok(Json(ListTenantsResponse {
    total_row_count: tenants.len(),
    tenants,
  }));
}
