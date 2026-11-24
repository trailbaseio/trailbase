pub mod create_tenant;
pub mod delete_tenant;
pub mod list_tenants;

#[cfg(test)]
mod tests {
  use axum::extract::{Json, RawQuery, State};
  use std::assert_matches;

  use super::*;
  use crate::admin::AdminError;
  use crate::admin::tenant::create_tenant::CreateTenantResponse;
  use crate::app_state::{TestStateOptions, test_state};
  use crate::config::proto;

  #[tokio::test]
  async fn admin_tenant_api() {
    let state = test_state(Some(TestStateOptions {
      config: Some(proto::Config {
        tenant_templates: vec![proto::TenantTemplateConfig {
          name: Some("test_template".to_string()),
        }],
        ..Default::default()
      }),
      ..Default::default()
    }))
    .await
    .unwrap();

    assert_matches!(
      create_tenant::create_tenant_handler(
        State(state.clone()),
        Json(create_tenant::CreateTenantRequest {
          template: "missing_template_name".to_string(),
          name: "tenant_name".to_string(),
        }),
      )
      .await,
      Err(AdminError::Precondition(_))
    );

    let Json(CreateTenantResponse { id }) = create_tenant::create_tenant_handler(
      State(state.clone()),
      Json(create_tenant::CreateTenantRequest {
        template: "test_template".to_string(),
        name: "tenant_name".to_string(),
      }),
    )
    .await
    .unwrap();

    let Json(list_tenants::ListTenantsResponse {
      total_row_count,
      tenants,
    }) = list_tenants::list_tenants_handler(State(state.clone()), RawQuery(None))
      .await
      .unwrap();

    assert_eq!(1, total_row_count);
    assert_eq!(1, tenants.len());
    assert_eq!(id, tenants[0].id);

    delete_tenant::delete_tenant_handler(
      State(state.clone()),
      Json(delete_tenant::DeleteTenantRequest {
        id,
        remove_database: Some(false),
      }),
    )
    .await
    .unwrap();

    let Json(list_tenants::ListTenantsResponse {
      total_row_count,
      tenants,
    }) = list_tenants::list_tenants_handler(State(state.clone()), RawQuery(None))
      .await
      .unwrap();

    assert_eq!(0, total_row_count);
    assert_eq!(0, tenants.len());
  }
}
