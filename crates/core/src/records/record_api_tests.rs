use trailbase_sqlite::params;

use crate::admin::user::create_user::create_user_for_test;
use crate::api::{UserIdentifier, login_with_password_for_test};
use crate::app_state::*;
use crate::config::proto::{ConflictResolutionStrategy, PermissionFlag, RecordApiConfig};
use crate::records::test_utils::*;

#[tokio::test]
async fn multi_tenant_record_api_create() {
  let config = {
    let mut config = test_config();

    // TODO: Configure tenant templates.

    config
  };

  let state = test_state(Some(TestStateOptions {
    config: Some(config),
    ..Default::default()
  }))
  .await
  .unwrap();

  let password = "Secret!1!!";
  let user_x_email = "user_x@bar.com";
  let user_x = create_user_for_test(&state, user_x_email, password)
    .await
    .unwrap();

  const TENANT_NAME: &str = "test";
  const TEMPLATE_NAME: &str = "test_template";

  let tenant_id: [u8; 16] = state
    .user_conn()
    .write_query_row_get(
      "INSERT INTO _tenant (name, template) VALUES ($1, $2) RETURNING id;",
      params!(TENANT_NAME, TEMPLATE_NAME),
      0,
    )
    .await
    .unwrap()
    .unwrap();

  state
    .user_conn()
    .execute(
      "INSERT INTO _tenant_member (tenant, user) VALUES ($1, $2);",
      params!(tenant_id, user_x.into_bytes()),
    )
    .await
    .unwrap();

  // FIXME: How do we inject templates for test? Write to TestData directory?
  let conn = state.conn();
  conn
    .execute_batch(format!(
      "CREATE TABLE simple (
         owner   {uuid} PRIMARY KEY CHECK(is_uuid(owner)) REFERENCES _user,
         value   INTEGER
       ) {strict};",
      strict = strict(conn),
      uuid = uuid_column(conn),
    ))
    .await
    .unwrap();

  state.rebuild_connection_metadata().await.unwrap();

  add_record_api_config(
    &state,
    RecordApiConfig {
      name: Some("simple_api".to_string()),
      table_name: Some("simple".to_string()),
      tenant_template: Some(TEMPLATE_NAME.to_string()),
      conflict_resolution: Some(ConflictResolutionStrategy::Replace as i32),
      acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
      create_access_rule: Some("_USER_.id = _REQ_.owner".to_string()),
      ..Default::default()
    },
  )
  .await
  .unwrap();

  let user_x_token = login_with_password_for_test(
    &state,
    UserIdentifier::Email(user_x_email.to_string()),
    Some(uuid::Uuid::from_bytes(tenant_id)),
    password,
  )
  .await
  .unwrap();
}
