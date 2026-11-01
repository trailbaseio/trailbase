use axum::extract::{Path, Query, RawQuery, State};
use serde_json::json;
use trailbase_sqlite::params;

use crate::app_state::*;
use crate::auth::User;
use crate::config::proto::{ConflictResolutionStrategy, PermissionFlag, RecordApiConfig};
use crate::extract::Either;
use crate::records::create_record::{CreateRecordQuery, create_record_handler};
use crate::records::list_records::{ListOrGeoJSONResponse, ListRecordsQuery, list_records_handler};
use crate::records::test_utils::*;
use crate::test_util::*;

async fn add_new_tenant_with_one_member(
  state: &AppState,
  email: &str,
  template_name: &str,
  tenant_name: &str,
) -> (uuid::Uuid, TestUserAndLogin) {
  let user = create_user_for_test(
    state,
    UserIdentifier::Email(email.to_string()),
    "Secret!1!!",
  )
  .await;

  let tenant_id = uuid::Uuid::from_bytes(
    state
      .user_conn()
      .write_query_row_get::<[u8; 16]>(
        "INSERT INTO _tenant (name, template) VALUES ($1, $2) RETURNING id;",
        params!(tenant_name.to_string(), template_name.to_string()),
        0,
      )
      .await
      .unwrap()
      .unwrap(),
  );

  state
    .user_conn()
    .execute(
      "INSERT INTO _tenant_member (tenant, user) VALUES ($1, $2);",
      params!(tenant_id.into_bytes(), user.id.into_bytes()),
    )
    .await
    .unwrap();

  let login =
    login_user_with_tenant_for_test(&state, user.user.clone(), Some(tenant_id), &user.password)
      .await
      .unwrap();

  {
    let user = User::from_auth_token(&state, &login.auth_token).unwrap();
    assert_eq!(tenant_id, user.tenant.as_ref().unwrap().id);
    assert_eq!(template_name, user.tenant.as_ref().unwrap().template);
  }

  return (tenant_id, TestUserAndLogin { user, login });
}

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

  const API_NAME: &str = "simple_api";
  const TEMPLATE_NAME: &str = "test_template";

  let (
    tenant_x,
    TestUserAndLogin {
      user: user_x,
      login: user_x_tokens,
    },
  ) = add_new_tenant_with_one_member(&state, "user_x@test.org", TEMPLATE_NAME, "tenant_x").await;
  let (
    tenant_y,
    TestUserAndLogin {
      user: user_y,
      login: user_y_tokens,
    },
  ) = add_new_tenant_with_one_member(&state, "user_y@test.org", TEMPLATE_NAME, "tenant_y").await;

  let template_migrations = state
    .data_dir()
    .root()
    .join("templates")
    .join(TEMPLATE_NAME);
  std::fs::create_dir_all(&template_migrations).unwrap();

  let conn = state.conn();
  std::fs::write(
    template_migrations.join("V0__simple.sql"),
    format!(
      "\
        CREATE TABLE simple ( \
         owner   {uuid} PRIMARY KEY CHECK(is_uuid(owner)), \
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
      name: Some(API_NAME.to_string()),
      table_name: Some(format!("{TEMPLATE_NAME}.simple")),
      tenant_template: Some(TEMPLATE_NAME.to_string()),
      conflict_resolution: Some(ConflictResolutionStrategy::Replace as i32),
      acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
      create_access_rule: Some("_USER_.id = _REQ_.owner".to_string()),
      ..Default::default()
    },
  )
  .await
  .unwrap();

  let _ = create_record_handler(
    State(state.clone()),
    Path(API_NAME.to_string()),
    Query(CreateRecordQuery::default()),
    Some(User::from_auth_token(&state, &user_x_tokens.auth_token).unwrap()),
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

  let ListOrGeoJSONResponse::List(list) = list_records_handler(
    State(state.clone()),
    Path(API_NAME.to_string()),
    Query(ListRecordsQuery::default()),
    RawQuery(Some("count=TRUE".to_string())),
    Some(User::from_auth_token(&state, &user_x_tokens.auth_token).unwrap()),
  )
  .await
  .unwrap()
  .0
  else {
    panic!("not a list");
  };

  assert_eq!(Some(1), list.total_count);

  // Make sure a user off a different tenant is isolated.
  let ListOrGeoJSONResponse::List(list) = list_records_handler(
    State(state.clone()),
    Path(API_NAME.to_string()),
    Query(ListRecordsQuery::default()),
    RawQuery(Some("count=TRUE".to_string())),
    Some(User::from_auth_token(&state, &user_y_tokens.auth_token).unwrap()),
  )
  .await
  .unwrap()
  .0
  else {
    panic!("not a list");
  };

  assert_eq!(Some(0), list.total_count);
}
