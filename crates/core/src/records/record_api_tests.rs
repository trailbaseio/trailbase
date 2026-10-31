use axum::extract::{Path, Query, State};
use serde_json::json;
use trailbase_sqlite::params;

use crate::app_state::*;
use crate::auth::User;
use crate::config::proto::{ConflictResolutionStrategy, PermissionFlag, RecordApiConfig};
use crate::extract::Either;
use crate::records::create_record::{CreateRecordQuery, create_record_handler};
use crate::records::test_utils::*;
use crate::test_util::*;

#[tokio::test]
#[ignore = "Our RecordApi ACL checking flow requires that 'main' always has the _user table. The multi-tenant DBs we construct don't have that and we first need to allow main and attached DB migrations from entirely different paths: /migrations/ vs /templates/"]
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
  let user_x = create_user_for_test(
    &state,
    UserIdentifier::Email(user_x_email.to_string()),
    password,
  )
  .await;

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
      params!(tenant_id, user_x.id.into_bytes()),
    )
    .await
    .unwrap();

  let conn = state.conn();
  let template_migrations = state
    .data_dir()
    .root()
    .join("templates")
    .join(TEMPLATE_NAME);
  std::fs::create_dir_all(&template_migrations).unwrap();
  let migration_path = template_migrations.join("V0__simple.sql");

  std::fs::write(
    migration_path,
    format!(
      "\
        CREATE TABLE simple ( \
         owner   {uuid} PRIMARY KEY CHECK(is_uuid(owner)) REFERENCES _user, \
         value   INTEGER \
         ) {strict}; \
      ",
      strict = strict(conn),
      uuid = uuid_column(conn),
    ),
  )
  .unwrap();

  // QUESTION: Rebuild is currently not needed since we don't read templates. Is this something we
  // should do?
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

  let (user_x, user_x_tokens) = {
    let tokens = login_user_with_tenant_for_test(
      &state,
      UserIdentifier::Email(user_x_email.to_string()),
      Some(uuid::Uuid::from_bytes(tenant_id)),
      password,
    )
    .await
    .unwrap();

    let user = User::from_auth_token(&state, &tokens.auth_token).unwrap();

    assert_eq!(
      uuid::Uuid::from_bytes(tenant_id),
      user.tenant.as_ref().unwrap().id,
    );
    assert_eq!(TEMPLATE_NAME, user.tenant.as_ref().unwrap().template);

    (user, tokens)
  };

  let _ = create_record_handler(
    State(state.clone()),
    Path("simple_api".to_string()),
    Query(CreateRecordQuery::default()),
    Some(user_x.clone()),
    Either::Json(
      json_row_from_value(json!({
        "owner": crate::util::uuid_to_b64(&user_x.id),
        "value": 0,
      }))
      .unwrap()
      .into(),
    ),
  )
  .await
  .unwrap();
}
