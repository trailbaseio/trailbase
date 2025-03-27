use axum::{
  extract::{Path, RawQuery, State},
  Json,
};
use indoc::formatdoc;
use itertools::Itertools;
use serde::Serialize;
use std::borrow::Cow;
use trailbase_sqlite::Value;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::listing::{
  build_filter_where_clause, limit_or_default, parse_query, Order, QueryParseResult, WhereClause,
};
use crate::records::query_builder::Expansions;
use crate::records::sql_to_json::{row_to_json, row_to_json_expand, rows_to_json_expand};
use crate::records::{Permission, RecordError};
use crate::util::uuid_to_b64;

/// JSON response containing the listed records.
#[derive(Debug, Serialize)]
pub struct ListResponse {
  /// Pagination cursor. Round-trip to get the next batch.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cursor: Option<String>,
  /// The total number of records matching the query.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub total_count: Option<usize>,
  /// Actual record data for records matching the query.
  pub records: Vec<serde_json::Value>,
}

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
) -> Result<Json<ListResponse>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  // WARN: We do different access checking here because the access rule is used as a filter query
  // on the table, i.e. no access -> empty results.
  api.check_table_level_access(Permission::Read, user.as_ref())?;

  let metadata = api.metadata();
  let Some(columns) = metadata.columns() else {
    return Err(RecordError::Internal("missing columns".into()));
  };
  let Some((pk_index, _pk_column)) = metadata.record_pk_column() else {
    return Err(RecordError::Internal("missing pk column".into()));
  };

  let QueryParseResult {
    params: filter_params,
    cursor,
    limit,
    order,
    count,
    expand: query_expand,
    ..
  } = parse_query(raw_url_query.as_deref()).map_err(|_err| {
    return RecordError::BadRequest("Invalid query");
  })?;

  // Where clause contains column filters and cursor depending on what's present.
  let WhereClause {
    mut clause,
    mut params,
  } = build_filter_where_clause(metadata, filter_params)
    .map_err(|_err| RecordError::BadRequest("Invalid filter params"))?;

  // User properties
  params.extend_from_slice(&[
    (
      Cow::Borrowed(":limit"),
      Value::Integer(limit_or_default(limit) as i64),
    ),
    (
      Cow::Borrowed(":__user_id"),
      user.map_or(Value::Null, |u| Value::Blob(u.uuid.into())),
    ),
  ]);

  // NOTE: We're using the read access rule to filter the rows as opposed to yes/no early access
  // blocking as for read-record.
  //
  // TODO: Should this be a separate access rule? Maybe one wants users to access a specific
  // record but not list all the records.
  if let Some(read_access) = api.access_rule(Permission::Read) {
    clause = format!("({read_access}) AND ({clause})");
  }

  let clause_with_cursor = match cursor {
    Some(cursor) => {
      params.push((Cow::Borrowed(":cursor"), cursor.into()));
      format!("{clause} AND _ROW_.id < :cursor")
    }
    None => clause.clone(),
  };

  let order_clause = order
    .unwrap_or_else(|| vec![(api.record_pk_column().name.clone(), Order::Descending)])
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
    .join(", ");

  let table_name = api.table_name();

  let (indexes, joins, selects) = match query_expand {
    Some(ref expand) if !expand.is_empty() => {
      let Some(config_expand) = api.expand() else {
        return Err(RecordError::BadRequest("Invalid expansion"));
      };

      for col_name in expand {
        if !config_expand.contains_key(col_name) {
          return Err(RecordError::BadRequest("Invalid expansion"));
        }
      }

      let Expansions {
        indexes,
        joins,
        selects,
      } = Expansions::build(state.table_metadata(), table_name, expand, Some("_ROW_"))?;

      (Some(indexes), Some(joins), selects)
    }
    _ => (None, None, None),
  };

  let get_total_count = count.unwrap_or(false);
  let query = if get_total_count {
    formatdoc!(
      r#"
      WITH total_count AS (
        SELECT COUNT(*) AS _value_
        FROM
          '{table_name}' as _ROW_,
          (SELECT :__user_id AS id) AS _USER_
        WHERE
          {clause}
      )

      SELECT
        _ROW_.*,
        {selects}
        total_count._value_
      FROM
        '{table_name}' as _ROW_ {joins},
        (SELECT :__user_id AS id) AS _USER_,
        total_count
      WHERE
        {clause_with_cursor}
      ORDER BY
        {order_clause}
      LIMIT :limit
      "#,
      selects = selects.map_or(EMPTY, |v| format!("{}, ", v.join(", "))),
      joins = joins.map_or(EMPTY, |j| j.join(" ")),
    )
  } else {
    formatdoc!(
      r#"
      SELECT
        _ROW_.*
        {selects}
      FROM
        '{table_name}' AS _ROW_ {joins},
        (SELECT :__user_id AS id) AS _USER_
      WHERE
        {clause_with_cursor}
      ORDER BY
        {order_clause}
      LIMIT :limit
      "#,
      selects = selects.map_or(EMPTY, |v| format!(", {}", v.join(", "))),
      joins = joins.map_or(EMPTY, |j| j.join(" ")),
    )
  };

  let rows = state.conn().query(&query, params).await?;
  let Some(last_row) = rows.last() else {
    // Rows are empty:
    return Ok(Json(ListResponse {
      cursor: None,
      total_count: Some(0),
      records: vec![],
    }));
  };

  assert!(pk_index < last_row.len());
  let cursor = match &last_row[pk_index] {
    rusqlite::types::Value::Blob(blob) => {
      uuid::Uuid::from_slice(blob).as_ref().map(uuid_to_b64).ok()
    }
    rusqlite::types::Value::Integer(i) => Some(i.to_string()),
    _ => None,
  };

  let total_count = if get_total_count {
    let Some(rusqlite::types::Value::Integer(count)) = rows[0].last() else {
      return Err(RecordError::Internal(
        format!("expected count, got {:?}", rows[0].last()).into(),
      ));
    };
    Some(*count as usize)
  } else {
    None
  };

  fn filter(col_name: &str) -> bool {
    return !col_name.starts_with("_");
  }

  let records = if let (Some(query_expand), Some(indexes)) = (query_expand, indexes) {
    rows
      .into_iter()
      .map(|row| {
        let Some(mut expand) = api.expand().cloned() else {
          return Err(RecordError::Internal(
            "Expansion config must be some".into(),
          ));
        };

        let mut curr = row;
        let mut result = Vec::with_capacity(indexes.len());
        for (idx, metadata) in &indexes {
          let next = curr.split_off(*idx);
          result.push((metadata, curr));
          curr = next;
        }

        let foreign_rows = result.split_off(1);
        for (col_name, (metadata, row)) in std::iter::zip(&query_expand, foreign_rows) {
          let foreign_value = row_to_json(
            &metadata.schema.columns,
            metadata.column_metadata(),
            &row,
            filter,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let result = expand.insert(col_name.to_string(), foreign_value);
          assert!(result.is_some());
        }

        return row_to_json_expand(
          columns,
          metadata.column_metadata(),
          &result[0].1,
          filter,
          Some(&expand),
        )
        .map_err(|err| RecordError::Internal(err.into()));
      })
      .collect::<Result<Vec<_>, RecordError>>()?
  } else {
    rows_to_json_expand(
      columns,
      metadata.column_metadata(),
      rows,
      filter,
      api.expand(),
    )
    .map_err(|err| RecordError::Internal(err.into()))?
  };

  return Ok(Json(ListResponse {
    cursor,
    total_count,
    records,
  }));
}

const EMPTY: String = String::new();

#[cfg(test)]
mod tests {
  use serde::Deserialize;

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

  #[allow(unused)]
  #[derive(Deserialize)]
  struct Message {
    id: String,
    _owner: Option<String>,
    room: String,
    data: String,
  }

  #[tokio::test]
  async fn test_record_api_list() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room0 = add_room(conn, "room0").await.unwrap();
    let room1 = add_room(conn, "room1").await.unwrap();
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
    .await.unwrap();

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
      .await
      .unwrap()
      .into_bytes();
    let user_x_token = login_with_password(&state, user_x_email, password)
      .await
      .unwrap();

    add_user_to_room(conn, user_x, room0).await.unwrap();
    send_message(conn, user_x, room0, "user_x to room0")
      .await
      .unwrap();

    let user_y_email = "user_y@foo.baz";
    let user_y = create_user_for_test(&state, user_y_email, password)
      .await
      .unwrap()
      .into_bytes();

    add_user_to_room(conn, user_y, room0).await.unwrap();
    send_message(conn, user_y, room0, "user_y to room0")
      .await
      .unwrap();

    add_user_to_room(conn, user_y, room1).await.unwrap();
    send_message(conn, user_y, room1, "user_y to room1")
      .await
      .unwrap();

    let user_y_token = login_with_password(&state, user_y_email, password)
      .await
      .unwrap();

    {
      // User X can list the messages they have access to, i.e. room0.
      let resp = list_records(&state, Some(&user_x_token.auth_token), None)
        .await
        .unwrap();

      assert_eq!(resp.records.len(), 2);

      let messages: Vec<_> = resp
        .records
        .into_iter()
        .map(|v| {
          match v {
            serde_json::Value::Object(ref obj) => {
              let keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
              assert_eq!(3, keys.len(), "Got: {:?}", keys);
              assert_eq!(keys, vec!["id", "room", "data"])
            }
            _ => panic!("expected object, got {v:?}"),
          };

          let message = serde_json::from_value::<Message>(v).unwrap();
          assert_eq!(None, message._owner);
          message.data
        })
        .collect();

      assert_eq!(
        vec!["user_y to room0".to_string(), "user_x to room0".to_string()],
        messages
      );
    }

    {
      // Test total count.
      //
      // User X can list the messages they have access to, i.e. room0.
      let resp = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some("count=TRUE".to_string()),
      )
      .await
      .unwrap();

      assert_eq!(resp.records.len(), 2);
      assert_eq!(resp.total_count, Some(2));

      let messages: Vec<_> = resp
        .records
        .into_iter()
        .map(|v| {
          match v {
            serde_json::Value::Object(ref obj) => {
              let keys: Vec<&str> = obj.keys().map(|s| s.as_str()).collect();
              assert_eq!(3, keys.len(), "Got: {:?}", keys);
              assert_eq!(keys, vec!["id", "room", "data"])
            }
            _ => panic!("expected object, got {v:?}"),
          };

          let message = serde_json::from_value::<Message>(v).unwrap();
          assert_eq!(None, message._owner);
          message.data
        })
        .collect();

      assert_eq!(
        vec!["user_y to room0".to_string(), "user_x to room0".to_string()],
        messages
      );

      // Let's paginate
      let resp0 = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some("count=1&limit=1".to_string()),
      )
      .await
      .unwrap();

      assert_eq!(resp0.records.len(), 1);
      assert_eq!(
        "user_y to room0",
        serde_json::from_value::<Message>(resp0.records[0].clone())
          .unwrap()
          .data
      );
      assert_eq!(resp0.total_count, Some(2));

      let cursor = resp0.cursor.unwrap();
      let resp1 = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some(format!("count=1&limit=1&cursor={cursor}")),
      )
      .await
      .unwrap();

      assert_eq!(resp1.records.len(), 1);
      assert_eq!(
        "user_x to room0",
        serde_json::from_value::<Message>(resp1.records[0].clone())
          .unwrap()
          .data
      );
      assert_eq!(resp1.total_count, Some(2));
      let cursor = resp1.cursor.unwrap();

      let resp2 = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some(format!("count=1&limit=1&cursor={cursor}")),
      )
      .await
      .unwrap();

      assert_eq!(resp2.records.len(), 0);
      assert!(resp2.cursor.is_none());
    }

    {
      // User Y can list the messages they have access to, i.e. room0 & room1.
      let arr = list_records(&state, Some(&user_y_token.auth_token), Some("".to_string()))
        .await
        .unwrap()
        .records;
      assert_eq!(arr.len(), 3);

      let arr = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("limit=1".to_string()),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr.len(), 1);
    }

    {
      // Ordering by id;
      let arr_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("order=+id".to_string()),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr_asc.len(), 3);

      let arr_desc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("order=-id".to_string()),
      )
      .await
      .unwrap()
      .records;
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
      .await
      .unwrap()
      .records;

      assert_eq!(arr0.len(), 2);

      let arr1 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("room={}", id_to_b64(&room1))),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr1.len(), 1);
    }
  }

  async fn list_records(
    state: &AppState,
    auth_token: Option<&str>,
    query: Option<String>,
  ) -> Result<ListResponse, RecordError> {
    let json_response = list_records_handler(
      State(state.clone()),
      Path("messages_api".to_string()),
      RawQuery(query),
      auth_token.and_then(|token| User::from_auth_token(&state, token)),
    )
    .await?;

    let response: ListResponse = json_response.0;
    return Ok(response);
  }
}
