use askama::Template;
use axum::{
  Json,
  extract::{Path, Query, RawQuery, State},
};
use base64::prelude::*;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::convert::TryInto;
use std::sync::LazyLock;
use trailbase_qs::OrderPrecedent;
use trailbase_schema::QualifiedNameEscaped;
use trailbase_schema::metadata::ColumnMetadata;
use trailbase_sqlite::Value;

use crate::app_state::AppState;
use crate::auth::user::User;
use crate::encryption::{KeyType, decrypt, encrypt, generate_random_key};
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::records::expand::{ExpandedTable, JsonError, expand_tables, row_to_json_expand};
use crate::records::{Permission, RecordError};

/// JSON response containing the listed records.
#[derive(Debug, Serialize)]
pub struct ListResponse {
  /// Encrypted cursor for pagination - Round-trip to get the next page.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cursor: Option<String>,
  /// The total number of records matching the query.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub total_count: Option<usize>,
  /// Actual record data for records matching the query.
  pub records: Vec<serde_json::Value>,
}

#[derive(Debug)]
pub enum ListOrGeoJSONResponse {
  List(ListResponse),
  GeoJSON(geos::geojson::FeatureCollection),
}

// Transparent serializer. We could probably have an Either instead.
impl Serialize for ListOrGeoJSONResponse {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: serde::Serializer,
  {
    return match self {
      Self::List(v) => v.serialize(serializer),
      Self::GeoJSON(v) => v.serialize(serializer),
    };
  }
}

#[derive(Debug, Default, Deserialize)]
pub struct ListRecordsQuery {
  pub geojson: Option<String>,
}

/// Lists records matching the given filters
#[utoipa::path(
  get,
  path = "/{name}",
  tag = "records",
  responses(
    (status = 200, description = "Matching records.")
  )
)]
pub async fn list_records_handler(
  State(state): State<AppState>,
  Path(api_name): Path<String>,
  Query(query): Query<ListRecordsQuery>,
  RawQuery(raw_url_query): RawQuery,
  user: Option<User>,
) -> Result<Json<ListOrGeoJSONResponse>, RecordError> {
  let Some(api) = state.lookup_record_api(&api_name) else {
    return Err(RecordError::ApiNotFound);
  };

  // WARN: We do different access checking here because the access rule is used as a filter query
  // on the table, i.e. no access -> empty results.
  api.check_table_level_access(Permission::Read, user.as_ref())?;

  let table_name = api.table_name();
  let pk_meta = api.record_pk_column();
  let pk_column = &pk_meta.column;
  let is_table = api.is_table();

  let geojson_geometry_column = if let Some(column) = query.geojson {
    let meta = api.column_metadata_by_name(&column).ok_or_else(|| {
      return RecordError::BadRequest("Invalid geometry column");
    })?;
    if !meta.is_geometry {
      return Err(RecordError::BadRequest("Invalid geometry column"));
    }

    Some(meta)
  } else {
    None
  };

  let trailbase_qs::Query {
    limit,
    cursor,
    count,
    expand: query_expand,
    order,
    filter: filter_params,
    offset,
  } = raw_url_query
    .as_ref()
    .map_or_else(
      || Ok(Default::default()),
      |query| trailbase_qs::Query::parse(query),
    )
    .map_err(|_err| {
      return RecordError::BadRequest("Invalid query");
    })?;

  // Where clause contains column filters and cursor depending on what's present.
  // NOTE: This will also drop any filters for unknown columns, thus avoiding SQL injections.
  let WhereClause {
    clause: filter_clause,
    mut params,
  } = build_filter_where_clause("_ROW_", api.columns(), filter_params)
    .map_err(|_err| RecordError::BadRequest("Invalid filter params"))?;

  let limit: usize =
    limit_or_default(limit, api.listing_hard_limit()).map_err(RecordError::BadRequest)?;

  // User properties
  params.extend_from_slice(&[
    (
      Cow::Borrowed(":__limit"),
      // NOTE: We want to query at least one record, otherwise limit=0 isn't very meaningful. It
      // may be used to query the total count.
      Value::Integer(limit.max(1) as i64),
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

  // NOTE: We lost the ability to cursor VIEWs when we moved to `_rowid_`. They currently only
  // support OFFSET. We could restore functionality where a cursor-able PK is included.
  // NOTE: We cannot use cursors if there's a custom order/sorting defined.
  // NOTE: Multiple order criteria only matter for non-unique columns, i.e. ordering on PK
  // and then on another column makes no difference.
  let supports_cursor = is_table
    && order
      .as_ref()
      .is_none_or(|o| o.columns.is_empty() || (pk_column.name == o.columns[0].0));
  let cursor_clause = if let Some(encrypted_cursor) = cursor {
    if !supports_cursor {
      return Err(RecordError::BadRequest(
        "Only TABLEs with PK primary order support cursors. Use offset instead.",
      ));
    }

    params.push((
      Cow::Borrowed(":cursor"),
      Value::Integer(decrypt_cursor(
        &EPHEMERAL_CURSOR_KEY,
        &api_name,
        &encrypted_cursor,
      )?),
    ));

    // We already check above that primary order criteria must be none or PK.
    match order.as_ref() {
      Some(ord)
        if ord
          .columns
          .first()
          .is_some_and(|(_c, o)| *o == OrderPrecedent::Ascending) =>
      {
        Some("_ROW_._rowid_ > :cursor".to_string())
      }
      // Descending:
      _ => Some("_ROW_._rowid_ < :cursor".to_string()),
    }
  } else {
    None
  };

  let order_clause = order.as_ref().map_or_else(
    || fmt_order(&pk_column.name, OrderPrecedent::Descending),
    |o| {
      o.columns
        .iter()
        .map(|(col, ord)| fmt_order(col, ord.clone()))
        .join(",")
    },
  );

  let metadata = api.connection_metadata();
  let expanded_tables = match query_expand {
    Some(expand) => {
      let Some(config_expand) = api.expand() else {
        return Err(RecordError::BadRequest("Invalid expansion"));
      };

      // NOTE: This will drop any unknown expand column, thus avoiding SQL injections.
      for col_name in &expand.columns {
        if !config_expand.contains_key(col_name) {
          return Err(RecordError::BadRequest("Invalid expansion"));
        }
      }

      expand_tables(&api, &metadata, &expand.columns)?
    }
    None => vec![],
  };

  // Execute the query.
  let rows = api
    .conn()
    .read_query_rows(
      // NOTE: The template relies on load-bearing underscores for "_rowid_" and "_total_count_" to
      // have them be stripped later on by `rows_to_json`.
      ListRecordQueryTemplate {
        table_name,
        column_names: &api
          .columns()
          .iter()
          .map(|meta| meta.column.name.as_str())
          .collect::<Vec<_>>(),
        // NOTE: We're using the read access rule to filter accessible rows as opposed to blocking
        // access early as we do for READs.
        read_access_clause: api.read_access_rule().unwrap_or("TRUE"),
        filter_clause: &filter_clause,
        cursor_clause: cursor_clause.as_deref(),
        order_clause: &order_clause,
        expanded_tables: &expanded_tables,
        count: count.unwrap_or(false),
        offset: offset.is_some(),
        is_table,
      }
      .render()
      .map_err(|err| RecordError::Internal(err.into()))?,
      params,
    )
    .await?;

  let Some(last_row) = rows.last() else {
    // Query result is empty:
    return Ok(Json(ListOrGeoJSONResponse::List(ListResponse {
      cursor: None,
      total_count: Some(0),
      records: vec![],
    })));
  };

  let total_count = if count == Some(true) {
    // Total count is in the final column for VIEWS and the penultimate for TABLES.
    let count_index = last_row.len() - if is_table { 2 } else { 1 };
    assert_eq!(rows.column_name(count_index), Some("_total_count_"));

    let value = &last_row[count_index];
    let Value::Integer(count) = value else {
      return Err(RecordError::Internal(
        format!("expected count, got {value:?}").into(),
      ));
    };

    Some(*count as usize)
  } else {
    None
  };

  // For ?limit=0 we still query one record to get the total count.
  if limit == 0 {
    return Ok(Json(ListOrGeoJSONResponse::List(ListResponse {
      cursor: None,
      total_count,
      records: vec![],
    })));
  }

  let cursor: Option<String> = if supports_cursor {
    // The SQL query template returns thw row id as the last column.
    let value = &last_row[last_row.len() - 1];
    let Value::Integer(rowid) = value else {
      return Err(RecordError::Internal(
        format!("expected integer cursor, got {value:?}").into(),
      ));
    };
    Some(encrypt_cursor(&EPHEMERAL_CURSOR_KEY, &api_name, *rowid)?)
  } else {
    None
  };

  let records = if expanded_tables.is_empty() {
    rows
      .into_iter()
      .map(|row| row_to_json_expand(api.columns(), &row, column_filter, api.expand()))
      .collect::<Result<Vec<_>, JsonError>>()
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

          let foreign_value = row_to_json_expand(
            &expanded.metadata.column_metadata,
            &curr,
            column_filter,
            None,
          )
          .map_err(|err| RecordError::Internal(err.into()))?;

          let result = expand.insert(expanded.local_column_name.clone(), foreign_value);
          assert!(result.is_some());

          curr = next;
        }

        return row_to_json_expand(api.columns(), &row, column_filter, Some(&expand))
          .map_err(|err| RecordError::Internal(err.into()));
      })
      .collect::<Result<Vec<_>, RecordError>>()?
  };

  if let Some(meta) = geojson_geometry_column {
    return Ok(Json(ListOrGeoJSONResponse::GeoJSON(
      build_feature_collection(meta, &pk_column.name, cursor, total_count, records)?,
    )));
  }

  return Ok(Json(ListOrGeoJSONResponse::List(ListResponse {
    cursor,
    total_count,
    records,
  })));
}

fn build_feature_collection(
  meta: &ColumnMetadata,
  pk_column_name: &str,
  cursor: Option<String>,
  total_count: Option<usize>,
  records: Vec<serde_json::Value>,
) -> Result<geos::geojson::FeatureCollection, RecordError> {
  let mut foreign_members = serde_json::Map::<String, serde_json::Value>::new();
  if let Some(cursor) = cursor {
    foreign_members.insert("cursor".to_string(), serde_json::Value::String(cursor));
  }
  if let Some(total_count) = total_count {
    foreign_members.insert("total_count".to_string(), serde_json::json!(total_count));
  }

  let features = records
    .into_iter()
    .map(|record| -> Result<geos::geojson::Feature, RecordError> {
      let serde_json::Value::Object(mut obj) = record else {
        return Err(RecordError::Internal("Not an object".into()));
      };

      let id = obj.get(pk_column_name).and_then(|id| match id {
        serde_json::Value::Number(n) => Some(geos::geojson::feature::Id::Number(n.clone())),
        serde_json::Value::String(s) => Some(geos::geojson::feature::Id::String(s.clone())),
        _ => None,
      });
      debug_assert!(id.is_some());

      // NOTE: Geometry may be NULL for nullable columns.
      let geometry = obj.remove(&meta.column.name).and_then(|g| {
        return geos::geojson::Geometry::from_json_value(g).ok();
      });

      return Ok(geos::geojson::Feature {
        id,
        geometry,
        properties: Some(obj),
        bbox: None,
        foreign_members: None,
      });
    })
    .collect::<Result<_, _>>()?;

  return Ok(geos::geojson::FeatureCollection {
    bbox: None,
    features,
    foreign_members: Some(foreign_members),
  });
}

fn fmt_order(col: &str, order: OrderPrecedent) -> String {
  return format!(
    r#"_ROW_."{col}" {}"#,
    match order {
      OrderPrecedent::Descending => "DESC",
      OrderPrecedent::Ascending => "ASC",
    }
  );
}

#[inline]
fn column_filter(col_name: &str) -> bool {
  return !col_name.starts_with("_");
}

/// Encrypts the cookie's value with authenticated encryption providing
/// confidentiality, integrity, and authenticity.
fn encrypt_cursor(key: &KeyType, api_name: &str, cursor: i64) -> Result<String, RecordError> {
  let input = cursor.to_string();

  let encrypted = encrypt(key, api_name.as_bytes(), input.as_bytes())
    .map_err(|_| RecordError::Internal("Failed to encode cursor".into()))?;

  return Ok(BASE64_URL_SAFE.encode(&encrypted));
}

fn decrypt_cursor(key: &KeyType, api_name: &str, encoded: &str) -> Result<i64, RecordError> {
  let cipher_text = BASE64_URL_SAFE
    .decode(encoded)
    .map_err(|_| RecordError::BadRequest("Bad cursor: b64"))?;

  let value = decrypt(key, api_name.as_bytes(), &cipher_text)
    .map_err(|_| RecordError::BadRequest("Bad cursor"))?;

  // For record ids we use the row_id, i.e. we expect this ot be an i64.
  return String::from_utf8_lossy(&value)
    .parse()
    .map_err(|_| RecordError::BadRequest("Bad cursor"));
}

#[derive(Template)]
#[template(escape = "none", path = "list_record_query.sql")]
struct ListRecordQueryTemplate<'a> {
  table_name: &'a QualifiedNameEscaped,
  column_names: &'a [&'a str],
  read_access_clause: &'a str,
  filter_clause: &'a str,
  cursor_clause: Option<&'a str>,
  order_clause: &'a str,
  expanded_tables: &'a [ExpandedTable<'a>],
  count: bool,
  offset: bool,
  is_table: bool,
}

// Ephemeral key for encrypting cursors, i.e. cursors cannot be re-used across TB restarts.
static EPHEMERAL_CURSOR_KEY: LazyLock<KeyType> = LazyLock::new(generate_random_key);

#[cfg(test)]
mod tests {
  use serde::Deserialize;
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::QualifiedName;
  use trailbase_sqlite::Value;

  use super::*;
  use crate::admin::user::*;
  use crate::app_state::*;
  use crate::auth::user::User;
  use crate::auth::util::login_with_password;
  use crate::config::proto::PermissionFlag;
  use crate::connection::ConnectionEntry;
  use crate::records::RecordError;
  use crate::records::test_utils::*;
  use crate::util::id_to_b64;
  use crate::util::urlencode;

  fn sanitize_template(template: &str) {
    assert!(parse_into_statement(template).is_ok(), "{template}");
    assert!(!template.contains("\n\n"), "{template}");
  }

  #[test]
  fn test_list_records_template() {
    sanitize_template(
      &ListRecordQueryTemplate {
        table_name: &QualifiedName::parse("table").unwrap().into(),
        column_names: &["a", "index"],
        read_access_clause: "TRUE",
        filter_clause: "TRUE",
        cursor_clause: Some("TRUE"),
        order_clause: "NULL",
        expanded_tables: &[],
        count: false,
        offset: false,
        is_table: true,
      }
      .render()
      .unwrap(),
    );

    sanitize_template(
      &ListRecordQueryTemplate {
        table_name: &QualifiedName {
          name: "table".to_string(),
          database_schema: Some("db".to_string()),
        }
        .into(),
        column_names: &["a", "index"],
        read_access_clause: "_USER_.id IS NOT NULL",
        filter_clause: "a = 'value'",
        cursor_clause: None,
        order_clause: "'index' ASC",
        expanded_tables: &[],
        count: true,
        offset: true,
        is_table: false,
      }
      .render()
      .unwrap(),
    );
  }

  #[test]
  fn test_cursor_encryption() {
    let api_name = "test_api";

    let key = generate_random_key();

    let value: i64 = 3298473294;
    let encrypted = encrypt_cursor(&key, api_name, value).unwrap();
    let decrypted = decrypt_cursor(&key, api_name, &encrypted).unwrap();

    assert_eq!(value, decrypted);
  }

  #[tokio::test]
  async fn test_list_records_template_with_expansions() {
    let state = test_state(None).await.unwrap();

    {
      let ConnectionEntry {
        connection: conn, ..
      } = state.connection_manager().main_entry();

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

      state.rebuild_connection_metadata().await.unwrap();
    }

    let ConnectionEntry {
      connection: conn,
      metadata: connection_metadata,
    } = state.connection_manager().main_entry();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("api".to_string()),
        table_name: Some("table".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let api = state.lookup_record_api("api").unwrap();

    let expanded_tables = expand_tables(&api, &connection_metadata, &["index"]).unwrap();

    assert_eq!(expanded_tables.len(), 1);
    assert_eq!(expanded_tables[0].local_column_name, "index");
    assert_eq!(expanded_tables[0].foreign_table_name, "other");
    assert_eq!(expanded_tables[0].foreign_column_name, "index");

    let query = ListRecordQueryTemplate {
      table_name: &QualifiedName {
        name: "table".to_string(),
        database_schema: Some("main".to_string()),
      }
      .into(),
      column_names: &["tid", "drop", "index"],
      read_access_clause: "_USER_.id != X'F000'",
      filter_clause: "TRUE",
      cursor_clause: None,
      order_clause: "tid",
      expanded_tables: &expanded_tables,
      count: true,
      offset: false,
      is_table: true,
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
      nullable: Option<i64>,
    }

    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
        CREATE TABLE 'table' (
          id INTEGER PRIMARY KEY,
          'index' TEXT NOT NULL DEFAULT '',
          nullable INTEGER
        ) STRICT;
        INSERT INTO 'table' (id, 'index', nullable) VALUES (1, '1', 1), (2, '2', NULL), (3, '3', NULL);
      "#,
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("api".to_string()),
        table_name: Some("table".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let ListOrGeoJSONResponse::List(response) = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(None),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(3, response.records.len());

    let first: Entry = serde_json::from_value(response.records[0].clone()).unwrap();

    let ListOrGeoJSONResponse::List(response) = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some(format!("filter[id]={}", first.id))),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(1, response.records.len());
    assert_eq!(
      first,
      serde_json::from_value(response.records[0].clone()).unwrap()
    );

    let ListOrGeoJSONResponse::List(null_response) = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some("filter[nullable][$is]=NULL".to_string())),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(2, null_response.records.len());

    let ListOrGeoJSONResponse::List(not_null_response) = list_records_handler(
      State(state.clone()),
      Path("api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some("filter[nullable][$is]=!NULL".to_string())),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };
    assert_eq!(1, not_null_response.records.len());
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
        Some(format!("filter[mid]={}", first.mid)),
      )
      .await
      .unwrap();

      assert_eq!(resp_by_id.records.len(), 1, "mid: {}", first.mid);
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

      // Edge case for limit=0. It's a convenient way to just get the total count of records.
      let resp = list_records(
        &state,
        Some(&user_x_token.auth_token),
        Some("count=TRUE&limit=0".to_string()),
      )
      .await
      .unwrap();

      assert_eq!(resp.records.len(), 0);
      assert_eq!(resp.total_count, Some(2));

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

      let limited_arr = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some("limit=1".to_string()),
      )
      .await
      .unwrap()
      .records;
      assert_eq!(limited_arr.len(), 1);

      // Composite filter
      let messages: Vec<Message> = arr.into_iter().map(to_message).collect();
      let first = &messages[0].mid;
      let third = &messages[2].mid;
      let filtered_arr = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!(
          "filter[$or][0][mid]={first}&filter[$or][1][mid][$eq]={third}"
        )),
      )
      .await
      .unwrap()
      .records;

      assert_eq!(filtered_arr.len(), 2);
      let filtd_messages: Vec<Message> = filtered_arr.into_iter().map(to_message).collect();
      assert_eq!(first, &filtd_messages[0].mid);
      assert_eq!(third, &filtd_messages[1].mid);
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
      let cursor_middle = list_records(
        &state,
        Some(&user_y_token.auth_token),
        // We're basically getting a cursor to the third element.
        Some("order=-mid&limit=2".to_string()),
      )
      .await
      .unwrap()
      .cursor
      .unwrap();

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

      // NOTE: Ascending ordering by PK column is no longer allowed, since PK might no longer
      // strictly monotonically increase like the _rowid_ by which we cursor.
      // let mut cursored_asc = list_records(
      //   &state,
      //   Some(&user_y_token.auth_token),
      //   Some(format!(
      //     "order={}&cursor={cursor_middle}",
      //     urlencode("+mid")
      //   )),
      // )
      // .await
      // .unwrap()
      // .records;
      //
      // assert_eq!(cursored_asc.len(), 1);
      // assert_eq!(
      //   to_message(cursored_asc.swap_remove(0)),
      //   to_message(arr_asc[2].clone())
      // );

      // Ordering and cursor return an error when PK is not primary order cirteria.
      assert!(
        list_records(
          &state,
          Some(&user_y_token.auth_token),
          Some(format!(
            "order={}&cursor={cursor_middle}",
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
        Some(format!("filter[room]={}", id_to_b64(&room0))),
      )
      .await
      .unwrap()
      .records;

      assert_eq!(arr0.len(), 2, "{arr0:?}");

      let arr1 = list_records(
        &state,
        Some(&user_y_token.auth_token),
        Some(format!("filter[room][$eq]={}", id_to_b64(&room1))),
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
      Query(ListRecordsQuery::default()),
      RawQuery(query),
      auth_token.and_then(|token| User::from_auth_token(&state, token)),
    )
    .await?;

    if let ListOrGeoJSONResponse::List(response) = json_response.0 {
      return Ok(response);
    };
    panic!("not a list response: {json_response:?}");
  }

  #[tokio::test]
  async fn test_record_api_list_view_api() {
    let state = test_state(None).await.unwrap();

    state
      .conn()
      .execute_batch(
        r#"
          CREATE TABLE data (
            id       INTEGER PRIMARY KEY,
            data     TEXT NOT NULL,
            flag     INTEGER NOT NULL DEFAULT 0
          ) STRICT;

          INSERT INTO data (id, data, flag) VALUES (0, 'msg0', 1), (1, 'msg1', 1), (2, 'msg2', 0);

          CREATE VIEW data_view AS SELECT * FROM data;

          CREATE VIEW data_view_filtered AS SELECT
              d.*,
              CAST(CONCAT('prefix_', d.data) AS TEXT) AS prefixed
            FROM data AS d
            WHERE d.flag > 0;
        "#,
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("data_view_api".to_string()),
        table_name: Some("data_view".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let ListOrGeoJSONResponse::List(resp) = list_records_handler(
      State(state.clone()),
      Path("data_view_api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some("count=TRUE".to_string())),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(3, resp.records.len());
    assert_eq!(3, resp.total_count.unwrap());

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some("data_view_filtered_api".to_string()),
        table_name: Some("data_view_filtered".to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    let ListOrGeoJSONResponse::List(resp_filtered0) = list_records_handler(
      State(state.clone()),
      Path("data_view_filtered_api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some("count=TRUE&offset=0".to_string())),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(2, resp_filtered0.records.len());
    assert_eq!(2, resp_filtered0.total_count.unwrap());

    let ListOrGeoJSONResponse::List(resp_filtered1) = list_records_handler(
      State(state.clone()),
      Path("data_view_filtered_api".to_string()),
      Query(ListRecordsQuery::default()),
      RawQuery(Some("count=TRUE&filter[prefixed]=prefix_msg0".to_string())),
      None,
    )
    .await
    .unwrap()
    .0
    else {
      panic!("not a list");
    };

    assert_eq!(1, resp_filtered1.records.len());
    assert_eq!(1, resp_filtered1.total_count.unwrap());
  }

  #[tokio::test]
  async fn test_record_api_geojson_list() {
    let state = test_state(None).await.unwrap();
    let name = "geometry";

    state
      .conn()
      .execute_batch(format!(
        r#"
        CREATE TABLE {name} (
          id            INTEGER PRIMARY KEY,
          description   TEXT,
          geom          BLOB CHECK(ST_IsValid(geom))
        ) STRICT;

        INSERT INTO {name} (id, description, geom) VALUES
          ( 3, 'Colloseo',     ST_GeomFromText('POINT(12.4924 41.8902)', 4326)),
          ( 7, 'A Line',       ST_GeomFromText('LINESTRING(10 20, 20 30)', 4326)),
          ( 8, 'br-quadrant',  ST_MakeEnvelope(0, -0, 180, -90)),
          (21, 'null',         NULL);
      "#
      ))
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    add_record_api_config(
      &state,
      RecordApiConfig {
        name: Some(name.to_string()),
        table_name: Some(name.to_string()),
        acl_world: [PermissionFlag::Read as i32].into(),
        ..Default::default()
      },
    )
    .await
    .unwrap();

    {
      let ListOrGeoJSONResponse::GeoJSON(response) = list_records_handler(
        State(state.clone()),
        Path(name.to_string()),
        Query(ListRecordsQuery {
          geojson: Some("geom".to_string()),
        }),
        RawQuery(None),
        None,
      )
      .await
      .unwrap()
      .0
      else {
        panic!("not GeoJSON");
      };

      assert_eq!(4, response.features.len());
    }

    {
      // Test that stored point is `within` a filter bounding box.
      let ListOrGeoJSONResponse::GeoJSON(response) = list_records_handler(
        State(state.clone()),
        Path(name.to_string()),
        Query(ListRecordsQuery {
          geojson: Some("geom".to_string()),
        }),
        RawQuery(Some(format!(
          "filter[geom][@within]={polygon}",
          // Colloseo @ 12.4924 41.8902
          polygon = urlencode("POLYGON ((12 40, 12 42, 13 42, 13 40, 12 40))")
        ))),
        None,
      )
      .await
      .unwrap()
      .0
      else {
        panic!("not GeoJSON");
      };

      assert_eq!(1, response.features.len());
      assert_eq!(
        Some(geos::geojson::feature::Id::Number(3.into())),
        response.features[0].id
      );
    }

    {
      // Test that stored polygon `contains` a filter point.
      let ListOrGeoJSONResponse::GeoJSON(response) = list_records_handler(
        State(state.clone()),
        Path(name.to_string()),
        Query(ListRecordsQuery {
          geojson: Some("geom".to_string()),
        }),
        RawQuery(Some(format!(
          "filter[geom][@contains]={point}",
          point = urlencode("POINT (12 -40)")
        ))),
        None,
      )
      .await
      .unwrap()
      .0
      else {
        panic!("not GeoJSON");
      };

      assert_eq!(1, response.features.len());
      assert_eq!(
        Some(geos::geojson::feature::Id::Number(8.into())),
        response.features[0].id
      );
    }
  }
}
