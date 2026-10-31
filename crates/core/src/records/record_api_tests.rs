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

  let user_x_password = "Secret!1!!";
  let user_x_email = "user_x@bar.com";
  let user_x = create_user_for_test(
    &state,
    UserIdentifier::Email(user_x_email.to_string()),
    user_x_password,
  )
  .await;

  const API_NAME: &str = "simple_api";
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

  // Log-in with user_x, then create a record and list it.
  let (user_x, user_x_tokens) = {
    let tokens = login_user_with_tenant_for_test(
      &state,
      UserIdentifier::Email(user_x_email.to_string()),
      Some(uuid::Uuid::from_bytes(tenant_id)),
      user_x_password,
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
    Path(API_NAME.to_string()),
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

  let ListOrGeoJSONResponse::List(list) = list_records_handler(
    State(state.clone()),
    Path(API_NAME.to_string()),
    Query(ListRecordsQuery::default()),
    RawQuery(Some("count=TRUE".to_string())),
    Some(user_x.clone()),
  )
  .await
  .unwrap()
  .0
  else {
    panic!("not a list");
  };

  assert_eq!(Some(1), list.total_count);
}
