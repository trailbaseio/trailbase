use axum::{
  extract::{Path, RawQuery, State},
  Json,
};

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::listing::{
  build_filter_where_clause, limit_or_default, parse_query, Order, WhereClause,
};
use crate::records::record_api::build_user_sub_select;
use crate::records::sql_to_json::rows_to_json;
use crate::records::{Permission, RecordError};

/// Lists records matching the given filters
#[utoipa::path(
  get,
  path = "/:name",
  responses(
    (status = 200, description = "Matching records.")
  )
)]
pub async fn list_records_handler(
  State(state): State<AppState>,
  Path(api_name): Path<String>,
  RawQuery(raw_url_query): RawQuery,
  user: Option<User>,
) -> Result<Json<serde_json::Value>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  // WARN: We do different access checking here because the access rule is used as a filter query
  // on the table, i.e. no access -> empty results.
  api.check_table_level_access(Permission::Read, user.as_ref())?;

  let (filter_params, cursor, limit, order) = match parse_query(raw_url_query) {
    Some(q) => (Some(q.params), q.cursor, q.limit, q.order),
    None => (None, None, None, None),
  };

  // Where clause contains column filters and cursor depending on what's present.
  let metadata = api.metadata();
  let WhereClause {
    mut clause,
    mut params,
  } = build_filter_where_clause(metadata, filter_params)
    .map_err(|_err| RecordError::BadRequest("Invalid filter params"))?;
  if let Some(cursor) = cursor {
    params.push((
      ":cursor".to_string(),
      trailbase_sqlite::Value::Blob(cursor.to_vec()),
    ));
    clause = format!("{clause} AND _ROW_.id < :cursor");
  }
  params.push((
    ":limit".to_string(),
    trailbase_sqlite::Value::Integer(limit_or_default(limit) as i64),
  ));

  // User properties
  let (user_sub_select, mut user_params) = build_user_sub_select(user.as_ref());
  params.append(&mut user_params);

  // NOTE: We're using the read access rule to filter the rows as opposed to yes/no early access
  // blocking as for read-record.
  //
  // TODO: Should this be a separate access rule? Maybe one wants users to access a specific
  // record but not list all the records.
  if let Some(read_access) = api.access_rule(Permission::Read) {
    clause = format!("({clause}) AND {read_access}");
  }

  let default_ordering = || {
    return vec![(api.record_pk_column().name.clone(), Order::Descending)];
  };

  let order_clause = order
    .unwrap_or_else(default_ordering)
    .iter()
    .map(|(col, ord)| {
      format!(
        "_ROW_.{col} {}",
        match ord {
          Order::Descending => "DESC",
          Order::Ascending => "ASC",
        }
      )
    })
    .collect::<Vec<_>>()
    .join(", ");

  let query = format!(
    r#"
      SELECT _ROW_.*
      FROM
        ({user_sub_select}) AS _USER_,
        (SELECT * FROM '{table_name}') as _ROW_
      WHERE
        {clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
    "#,
    table_name = api.table_name()
  );

  let rows = state.conn().query(&query, params).await?;

  return Ok(Json(serde_json::Value::Array(
    rows_to_json(metadata, rows, |col_name| !col_name.starts_with("_"))
      .await
      .map_err(|err| RecordError::Internal(err.into()))?,
  )));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::User;
  use crate::config::proto::PermissionFlag;
  use crate::records::test_utils::*;
  use crate::records::Acls;
  use crate::records::{add_record_api, AccessRules, RecordError};
  use crate::util::id_to_b64;

  fn is_auth_err(error: &RecordError) -> bool {
    return match error {
      RecordError::Forbidden => true,
      _ => false,
    };
  }

  #[tokio::test]
  async fn test_record_api_list() -> Result<(), anyhow::Error> {
    let state = test_state(None).await?;
    let conn = state.conn();

    create_chat_message_app_tables(&state).await?;
    let room0 = add_room(conn, "room0").await?;
    let room1 = add_room(conn, "room1").await?;
    let password = "Secret!1!!";

    add_record_api(
      &state,
      "messages_api",
      "message",
      Acls {
        authenticated: vec![PermissionFlag::Create, PermissionFlag::Read],
        ..Default::default()
      },
      AccessRules {
        read: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND user = _USER_.id))".to_string()),
        ..Default::default()
      },
    )
    .await?;

    {
      // Unauthenticated users cannot list
      let response = list_records(&state, None, None).await;
      assert!(
        is_auth_err(response.as_ref().err().unwrap()),
        "{response:?}"
      );
    }

    let user_x_email = "user_x@test.com";
    let user_x = create_user_for_test(&state, user_x_email, password)
      .await?
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password).await?;

    add_user_to_room(conn, user_x, room0).await?;
    send_message(conn, user_x, room0, "user_x to room0").await?;

    let user_y_email = "user_y@foo.baz";
    let user_y = create_user_for_test(&state, user_y_email, password)
      .await?
      .into_bytes();

    add_user_to_room(conn, user_y, room0).await?;
    send_message(conn, user_y, room0, "user_y to room0").await?;

    add_user_to_room(conn, user_y, room1).await?;
    send_message(conn, user_y, room1, "user_y to room1").await?;

    let user_y_token = login_with_password(&state, user_y_email, password).await?;

    {
      // User X can list the messages they have access to, i.e. room0.
      let arr = list_records(&state, Some(&user_x_token.auth_token), None).await?;
      assert_eq!(arr.len(), 2);
    }

    {
      // User Y can list the messages they have access to, i.e. room0 & room1.
      let arr = list_records(&state, Some(&user_y_token.auth_token), Some("".to_string())).await?;
      assert_eq!(arr.len(), 3);

      let arr = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("limit=1".to_string()),
      )
      .await?;
      assert_eq!(arr.len(), 1);
    }

    {
      // Ordering by id;
      let arr_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("order=+id".to_string()),
      )
      .await?;
      assert_eq!(arr_asc.len(), 3);

      let arr_desc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("order=-id".to_string()),
      )
      .await?;
      assert_eq!(arr_desc.len(), 3);

      assert_eq!(arr_asc, arr_desc.into_iter().rev().collect::<Vec<_>>());
    }

    {
      // Filter by room
      let arr0 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("room={}", id_to_b64(&room0))),
      )
      .await?;
      assert_eq!(arr0.len(), 2);

      let arr1 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("room={}", id_to_b64(&room1))),
      )
      .await?;
      assert_eq!(arr1.len(), 1);
    }

    return Ok(());
  }

  async fn list_records(
    state: &AppState,
    auth_token: Option<&str>,
    query: Option<String>,
  ) -> Result<Vec<serde_json::Value>, RecordError> {
    let response = list_records_handler(
      State(state.clone()),
      Path("messages_api".to_string()),
      RawQuery(query),
      auth_token.and_then(|token| User::from_auth_token(&state, token)),
    )
    .await?;

    let json = response.0;
    if let serde_json::Value::Array(arr) = json {
      return Ok(arr);
    }
    return Err(RecordError::BadRequest("Not a json array"));
  }
}
