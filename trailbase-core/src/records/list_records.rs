use askama::Template;
use axum::{
  Json,
  extract::{Path, RawQuery, State},
};
use itertools::Itertools;
use serde::Serialize;
use std::borrow::Cow;
use trailbase_sqlite::Value;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::listing::{
  Order, QueryParseResult, WhereClause, build_filter_where_clause, limit_or_default,
  parse_and_sanitize_query,
};
use crate::records::query_builder::{ExpandedTable, expand_tables};
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

#[derive(Template)]
#[template(escape = "none", path = "list_record_query.sql")]
struct ListRecordQueryTemplate<'a> {
  table_name: &'a str,
  column_names: &'a [&'a str],
  read_access_clause: &'a str,
  filter_clause: &'a str,
  cursor_clause: Option<&'a str>,
  order_clause: &'a str,
  expanded_tables: &'a [ExpandedTable],
  count: bool,
  offset: bool,
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

  let table_name = api.table_name();
  let (pk_index, pk_column) = api.record_pk_column();

  let QueryParseResult {
    limit,
    cursor,
    count,
    expand: query_expand,
    order,
    params: filter_params,
    offset,
  } = parse_and_sanitize_query(raw_url_query.as_deref()).map_err(|_err| {
    return RecordError::BadRequest("Invalid query");
  })?;

  // NOTE: We're using the read access rule to filter the rows as opposed to yes/no early access
  // blocking as for read-record.
  //
  // TODO: Should this be a separate access rule? Maybe one wants users to access a specific
  // record but not list all the records.
  let read_access_clause: &str = api.read_access_rule().unwrap_or("TRUE");

  // Where clause contains column filters and cursor depending on what's present.
  // NOTE: This will also drop any filters for unknown columns, thus avoiding SQL injections.
  let WhereClause {
    clause: filter_clause,
    mut params,
  } = build_filter_where_clause("_ROW_", api.columns(), filter_params)
    .map_err(|_err| RecordError::BadRequest("Invalid filter params"))?;

  // User properties
  params.extend_from_slice(&[
    (
      Cow::Borrowed(":__limit"),
      Value::Integer(limit_or_default(limit).map_err(RecordError::BadRequest)? as i64),
    ),
    (
      Cow::Borrowed(":__user_id"),
      user.map_or(Value::Null, |u| Value::Blob(u.uuid.into())),
    ),
  ]);

  if let Some(offset) = offset {
    params.push((
      Cow::Borrowed(":__offset"),
      Value::Integer(
        offset
          .try_into()
          .map_err(|_| RecordError::BadRequest("Invalid offset"))?,
      ),
    ));
  }

  let cursor_clause = if let Some(cursor) = cursor {
    let mut pk_order = Order::Descending;
    if let Some(ref order) = order {
      if let Some((col, ord)) = order.first() {
        if *col != pk_column.name {
          return Err(RecordError::BadRequest(
            "Cannot cursor on queries where the primary order criterion is not the primary key",
          ));
        }

        pk_order = ord.clone();
      }
    }

    params.push((Cow::Borrowed(":cursor"), cursor.into()));
    match pk_order {
      Order::Descending => Some(format!(r#"_ROW_."{}" < :cursor"#, pk_column.name)),
      Order::Ascending => Some(format!(r#"_ROW_."{}" > :cursor"#, pk_column.name)),
    }
  } else {
    None
  };

  fn fmt_order(col: &str, order: Order) -> String {
    return format!(
      r#"_ROW_."{col}" {}"#,
      match order {
        Order::Descending => "DESC",
        Order::Ascending => "ASC",
      }
    );
  }

  let order_clause = order.map_or_else(
    || fmt_order(&pk_column.name, Order::Descending),
    |order| {
      order
        .into_iter()
        .map(|(col, ord)| fmt_order(&col, ord))
        .join(",")
    },
  );

  let expanded_tables = match query_expand {
    Some(ref expand) => {
      let Some(config_expand) = api.expand() else {
        return Err(RecordError::BadRequest("Invalid expansion"));
      };

      // NOTE: This will drop any unknown expand column, thus avoiding SQL injections.
      for col_name in expand {
        if !config_expand.contains_key(col_name) {
          return Err(RecordError::BadRequest("Invalid expansion"));
        }
      }

      expand_tables(
        state.schema_metadata(),
        |column_name| {
          api
            .column_index_by_name(column_name)
            .map(|idx| &api.columns()[idx])
        },
        expand,
      )?
    }
    None => vec![],
  };

  // NOTE: the `total_count._value_` underscore is load-bearing to strip it from result based on
  // "_" prefix.
  let column_names: Vec<_> = api.columns().iter().map(|c| c.name.as_str()).collect();
  let query = ListRecordQueryTemplate {
    table_name,
    column_names: &column_names,
    read_access_clause,
    filter_clause: &filter_clause,
    cursor_clause: cursor_clause.as_deref(),
    order_clause: &order_clause,
    expanded_tables: &expanded_tables,
    count: count.unwrap_or(false),
    offset: offset.is_some(),
  }
  .render()
  .map_err(|err| RecordError::Internal(err.into()))?;

  // Execute the query.
  let rows = state.conn().read_query_rows(query, params).await?;
  let Some(last_row) = rows.last() else {
    // Rows are empty:
    return Ok(Json(ListResponse {
      cursor: None,
      total_count: Some(0),
      records: vec![],
    }));
  };

  assert!(*pk_index < last_row.len());
  let cursor = match &last_row[*pk_index] {
    rusqlite::types::Value::Blob(blob) => {
      uuid::Uuid::from_slice(blob).as_ref().map(uuid_to_b64).ok()
    }
    rusqlite::types::Value::Integer(i) => Some(i.to_string()),
    _ => None,
  };

  let total_count = if count == Some(true) {
    let Some(rusqlite::types::Value::Integer(count)) = rows[0].last() else {
      return Err(RecordError::Internal(
        format!("expected count, got {:?}", rows[0].last()).into(),
      ));
    };
    Some(*count as usize)
  } else {
    None
  };

  let records = if expanded_tables.is_empty() {
    rows_to_json_expand(
      api.columns(),
      api.json_column_metadata(),
      rows,
      column_filter,
      api.expand(),
    )
    .map_err(|err| RecordError::Internal(err.into()))?
  } else {
    rows
      .into_iter()
      .map(|mut row| {
        // Allocate new empty expansion map.
        let Some(mut expand) = api.expand().cloned() else {
          return Err(RecordError::Internal(
            "Expansion config must be some".into(),
          ));
        };

        let mut curr = row.split_off(api.columns().len());

        for expanded in &expanded_tables {
          let next = curr.split_off(expanded.num_columns);

          let foreign_value = row_to_json(
            &expanded.metadata.schema.columns,
            &expanded.metadata.json_metadata.columns,
            &curr,
            column_filter,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let result = expand.insert(expanded.local_column_name.clone(), foreign_value);
          assert!(result.is_some());

          curr = next;
        }

        return row_to_json_expand(
          api.columns(),
          api.json_column_metadata(),
          &row,
          column_filter,
          Some(&expand),
        )
        .map_err(|err| RecordError::Internal(err.into()));
      })
      .collect::<Result<Vec<_>, RecordError>>()?
  };

  return Ok(Json(ListResponse {
    cursor,
    total_count,
    records,
  }));
}

#[inline]
fn column_filter(col_name: &str) -> bool {
  return !col_name.starts_with("_");
}

#[cfg(test)]
mod tests {
  use serde::Deserialize;
  use std::borrow::Cow;
  use trailbase_schema::sqlite::sqlite3_parse_into_statement;
  use trailbase_sqlite::Value;

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::api::login::login_with_password;
  use crate::auth::user::User;
  use crate::config::proto::PermissionFlag;
  use crate::records::RecordError;
  use crate::records::query_builder::expand_tables;
  use crate::records::test_utils::*;
  use crate::schema_metadata::SchemaMetadataCache;
  use crate::util::id_to_b64;
  use crate::util::urlencode;

  fn sanitize_template(template: &str) {
    assert!(sqlite3_parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
  }

  #[test]
  fn test_list_records_template() {
    sanitize_template(
      &ListRecordQueryTemplate {
        table_name: "table",
        column_names: &["a", "index"],
        read_access_clause: "TRUE",
        filter_clause: "TRUE",
        cursor_clause: Some("TRUE"),
        order_clause: "NULL",
        expanded_tables: &[],
        count: false,
        offset: false,
      }
      .render()
      .unwrap(),
    );

    sanitize_template(
      &ListRecordQueryTemplate {
        table_name: "table",
        column_names: &["a", "index"],
        read_access_clause: "_USER_.id IS NOT NULL",
        filter_clause: "a = 'value'",
        cursor_clause: None,
        order_clause: "'index' ASC",
        expanded_tables: &[],
        count: true,
        offset: true,
      }
      .render()
      .unwrap(),
    );
  }

  #[tokio::test]
  async fn test_list_records_template_with_expansions() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute(
        r#"CREATE TABLE "other" (
              "index" INTEGER PRIMARY KEY
            ) STRICT"#,
        (),
      )
      .await
      .unwrap();

    conn
      .execute(
        r#"CREATE TABLE "table" (
              tid     INTEGER PRIMARY KEY,
              "drop"  TEXT,
              "index" INTEGER REFERENCES "other"("index")
            ) STRICT"#,
        (),
      )
      .await
      .unwrap();

    let schema_metadata = SchemaMetadataCache::new(conn.clone()).await.unwrap();
    let table_metadata = schema_metadata.get_table("table").unwrap();
    let expanded_tables = expand_tables(
      &schema_metadata,
      |column_name| table_metadata.column_by_name(column_name).map(|(_, c)| c),
      &["index"],
    )
    .unwrap();

    assert_eq!(expanded_tables.len(), 1);
    assert_eq!(expanded_tables[0].local_column_name, "index");
    assert_eq!(expanded_tables[0].foreign_table_name, "other");
    assert_eq!(expanded_tables[0].foreign_column_name, "index");

    let query = ListRecordQueryTemplate {
      table_name: "table",
      column_names: &["tid", "drop", "index"],
      read_access_clause: "_USER_.id != X'F000'",
      filter_clause: "TRUE",
      cursor_clause: None,
      order_clause: "tid",
      expanded_tables: &expanded_tables,
      count: true,
      offset: false,
    }
    .render()
    .unwrap();

    sanitize_template(&query);

    let params = vec![
      (Cow::Borrowed(":__limit"), Value::Integer(100)),
      (
        Cow::Borrowed(":__user_id"),
        Value::Blob(uuid::Uuid::now_v7().into()),
      ),
    ];

    let result = conn.read_query_rows(query, params).await;
    if let Err(err) = result {
      panic!("ERROR: {err}");
    }
  }

  fn is_auth_err(error: &RecordError) -> bool {
    return match error {
      RecordError::Forbidden => true,
      _ => false,
    };
  }

  #[tokio::test]
  async fn test_record_api_list() {
    #[derive(Debug, PartialEq, Deserialize)]
    struct Entry {
      id: i64,
      index: String,
    }

    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
        CREATE TABLE 'table' (
          id INTEGER PRIMARY KEY,
          'index' TEXT NOT NULL DEFAULT ''
        );
        INSERT INTO 'table' (id, 'index') VALUES (1, '1'), (2, '2'), (3, '3');
      "#,
      )
      .await
      .unwrap();

    state.schema_metadata().invalidate_all().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("api".to_string()),
        table_name: Some("table".to_string()),
        acl_world: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let response = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      RawQuery(None),
      None,
    )
    .await
    .unwrap()
    .0;

    assert_eq!(3, response.records.len());

    let first: Entry = serde_json::from_value(response.records[0].clone()).unwrap();

    let response = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      RawQuery(Some(format!("id={}", first.id))),
      None,
    )
    .await
    .unwrap()
    .0;

    assert_eq!(1, response.records.len());
    assert_eq!(
      first,
      serde_json::from_value(response.records[0].clone()).unwrap()
    );
  }

  #[tokio::test]
  async fn test_record_api_list_messages_api() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    create_chat_message_app_tables(&state).await.unwrap();
    let room0 = add_room(conn, "room0").await.unwrap();
    let room1 = add_room(conn, "room1").await.unwrap();
    let password = "Secret!1!!";

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("messages_api".to_string()),
        table_name: Some("message".to_string()),
        acl_authenticated: [PermissionFlag::Create as i32, PermissionFlag::Read as i32].into(),
        read_access_rule: Some("(_ROW_._owner = _USER_.id OR EXISTS(SELECT 1 FROM room_members WHERE room = _ROW_.room AND user = _USER_.id))".to_string()),
        ..Default::default()
      },
    )
    .await
    .unwrap();

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

      let messages: Vec<Message> = resp.records.into_iter().map(to_message).collect();

      assert_eq!(
        &["user_y to room0", "user_x to room0"],
        messages
          .iter()
          .map(|m| m.data.as_str())
          .collect::<Vec<_>>()
          .as_slice(),
      );

      let first = &messages[0];
      let resp_by_id = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some(format!("mid={}", first.mid)),
      )
      .await
      .unwrap();

      assert_eq!(resp_by_id.records.len(), 1);
      assert_eq!(*first, to_message(resp_by_id.records[0].clone()));
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

      let messages: Vec<Message> = resp.records.into_iter().map(to_message).collect();

      assert_eq!(
        &["user_y to room0", "user_x to room0"],
        messages
          .iter()
          .map(|m| m.data.as_str())
          .collect::<Vec<_>>()
          .as_slice(),
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
      assert_eq!("user_y to room0", to_message(resp0.records[0].clone()).data);
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
      assert_eq!("user_x to room0", to_message(resp1.records[0].clone()).data);
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
      // Filter by column with name that needs escaping.
      let arr_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("table=0")),
      )
      .await
      .unwrap()
      .records;
      assert!(arr_asc.len() > 0);
    }

    {
      // Offset
      let result0 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("offset=0")),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(result0.len(), 3);

      let result1 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("offset=1")),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(result1.len(), 2);
    }

    {
      // Ordering by message id;
      let arr_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("order={}", urlencode("+mid"))),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr_asc.len(), 3);

      // Ordering by 'table', which needs proper escaping;
      let arr_table_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("order={}", urlencode("+table"))),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr_table_asc.len(), 3);

      // The following assertion would be flaky in case the UUIDv7 message IDs were minted in the
      // same time slot.
      // assert!(to_message(arr_asc[0].clone()).mid < to_message(arr_asc[1].clone()).mid);

      let arr_desc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("order=-mid".to_string()),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(arr_desc.len(), 3);

      assert_eq!(arr_asc, arr_desc.into_iter().rev().collect::<Vec<_>>());

      // Ordering and cursor work well together.
      let cursor_middle = to_message(arr_asc[1].clone()).mid;

      let mut cursored_desc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!(
          "order={}&cursor={cursor_middle}",
          urlencode("-mid")
        )),
      )
      .await
      .unwrap()
      .records;

      assert_eq!(cursored_desc.len(), 1);
      assert_eq!(
        to_message(cursored_desc.swap_remove(0)),
        to_message(arr_asc[0].clone())
      );

      let mut cursored_asc = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!(
          "order={}&cursor={cursor_middle}",
          urlencode("+mid")
        )),
      )
      .await
      .unwrap()
      .records;

      assert_eq!(cursored_asc.len(), 1);
      assert_eq!(
        to_message(cursored_asc.swap_remove(0)),
        to_message(arr_asc[2].clone())
      );

      // Ordering and cursor return an error when PK is not primary order cirteria.
      let cursor_first = to_message(arr_asc[0].clone()).mid;
      assert!(
        list_records(
          &state,
          Some(&user_y_token.auth_token),
          Some(format!(
            "order={}&cursor={cursor_first}",
            urlencode("+room,+mid")
          )),
        )
        .await
        .is_err()
      );
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
