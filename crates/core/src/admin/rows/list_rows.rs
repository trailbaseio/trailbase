use axum::extract::{Json, Path, RawQuery, State};
use log::*;
use serde::Serialize;
use std::borrow::Cow;
use trailbase_qs::{Cursor, CursorType, Order, OrderPrecedent, Query};
use trailbase_schema::QualifiedName;
use trailbase_schema::sqlite::{Column, ColumnDataType};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::admin::util::rows_to_sql_value_rows;
use crate::app_state::AppState;
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListRowsResponse {
  /// Column schema.
  pub columns: Vec<Column>,

  /// Actual row data.
  pub rows: Vec<Vec<SqlValue>>,

  /// Total number of records.
  pub total_row_count: i64,
  /// Next cursor for pagination.
  pub cursor: Option<String>,
}

pub async fn list_rows_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListRowsResponse>, Error> {
  let Query {
    limit,
    cursor,
    order,
    filter: filter_params,
    offset,
    ..
  } = raw_url_query
    .as_ref()
    .map_or_else(|| Ok(Query::default()), |query| Query::parse(query))
    .map_err(|err| {
      return Error::BadRequest(format!("Invalid query '{err}': {raw_url_query:?}").into());
    })?;

  let table_name = QualifiedName::parse(&table_name)?;
  let conn = super::build_connection(&state, &table_name)?;
  let metadata = super::build_connection_metadata(&state, &conn, &table_name).await?;

  let Some(table_or_view) = metadata.get_table_or_view(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table or view '{table_name:?}' not found"
    )));
  };
  let qualified_name = table_or_view.qualified_name();

  // Where clause contains column filters and cursor depending on what's present in the url query
  // string.
  let filter_where_clause = if let Some(columns) = table_or_view.columns() {
    build_filter_where_clause("_ROW_", columns, filter_params)?
  } else {
    debug!("Filter clauses currently not supported for complex views");

    WhereClause {
      clause: "TRUE".to_string(),
      params: vec![],
    }
  };

  let total_row_count: i64 = {
    let where_clause = &filter_where_clause.clause;
    let count_query = format!(
      "SELECT COUNT(*) FROM {table} AS _ROW_ WHERE {where_clause}",
      table = qualified_name.escaped_string()
    );
    conn
      .read_query_row_f(count_query, filter_where_clause.params.clone(), |row| {
        row.get(0)
      })
      .await?
      .unwrap_or(-1)
  };

  let cursor_column = table_or_view.record_pk_column();
  let cursor = match (cursor, cursor_column) {
    (Some(cursor), Some((_idx, c))) => Some(parse_cursor(&cursor, c)?),
    _ => None,
  };
  let (rows, columns) = fetch_rows(
    &conn,
    qualified_name,
    filter_where_clause,
    &order,
    Pagination {
      cursor_column: cursor_column.map(|(_idx, c)| c),
      cursor,
      offset,
      limit: limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))?,
    },
  )
  .await?;

  let next_cursor = cursor_column.and_then(|(col_idx, _col)| {
    let row = rows.last()?;
    assert!(row.len() > col_idx);
    match &row[col_idx] {
      SqlValue::Integer(n) => Some(n.to_string()),
      SqlValue::Blob(b) => {
        // Should be a base64 encoded [u8; 16] id.
        b.to_b64_url_safe().ok()
      }
      _ => None,
    }
  });

  return Ok(Json(ListRowsResponse {
    total_row_count,
    cursor: next_cursor,
    // NOTE: in the view case we don't have a good way of extracting the columns from the "CREATE
    // VIEW" query so we fall back to columns constructed from the returned data.
    columns: match table_or_view.columns() {
      Some(schema_columns) if !schema_columns.is_empty() => schema_columns.to_vec(),
      _ => {
        // VIRTUAL TABLE or VIEW case.
        debug!("Falling back to inferred cols for view: {table_name:?}");
        columns
      }
    },
    rows,
  }));
}

struct Pagination<'a> {
  cursor_column: Option<&'a Column>,
  cursor: Option<Cursor>,
  offset: Option<usize>,
  limit: usize,
}

async fn fetch_rows(
  conn: &trailbase_sqlite::Connection,
  qualified_name: &QualifiedName,
  filter_where_clause: WhereClause,
  order: &Option<Order>,
  pagination: Pagination<'_>,
) -> Result<(Vec<Vec<SqlValue>>, Vec<Column>), Error> {
  let WhereClause {
    mut clause,
    mut params,
  } = filter_where_clause;

  params.extend_from_slice(&[
    (
      Cow::Borrowed(":limit"),
      trailbase_sqlite::Value::Integer(pagination.limit as i64),
    ),
    (
      Cow::Borrowed(":offset"),
      trailbase_sqlite::Value::Integer(pagination.offset.unwrap_or(0) as i64),
    ),
  ]);

  if let (Some(cursor), Some(pk_column)) = (pagination.cursor, pagination.cursor_column) {
    params.push((
      Cow::Borrowed(":cursor"),
      crate::admin::util::cursor_to_value(cursor),
    ));
    clause = format!(r#"{clause} AND _ROW_."{}" < :cursor"#, pk_column.name);
  }

  let order_clause = match order {
    Some(order) => order
      .columns
      .iter()
      .map(|(col, ord)| {
        format!(
          r#"_ROW_."{col}" {}"#,
          match ord {
            OrderPrecedent::Descending => "DESC",
            OrderPrecedent::Ascending => "ASC",
          }
        )
      })
      .collect::<Vec<_>>()
      .join(", "),
    None => match pagination.cursor_column {
      Some(col) => format!(r#""{col_name}" DESC"#, col_name = col.name),
      None => "NULL".to_string(),
    },
  };

  let query = format!(
    r#"
      SELECT _ROW_.* FROM {table} AS _ROW_
        WHERE
          {clause}
        ORDER BY
          {order_clause}
        LIMIT :limit
        OFFSET :offset
    "#,
    table = qualified_name.escaped_string()
  );

  let result_rows = conn
    .read_query_rows(query.clone(), params)
    .await
    .map_err(|err| {
      #[cfg(debug_assertions)]
      error!("QUERY: {query}\n\t=> {err}");

      return err;
    })?;

  return Ok((
    rows_to_sql_value_rows(&result_rows)?,
    crate::admin::util::rows_to_columns(&result_rows),
  ));
}

fn parse_cursor(cursor: &str, pk_col: &Column) -> Result<Cursor, Error> {
  return match pk_col.data_type {
    ColumnDataType::Blob => {
      Cursor::parse(cursor, CursorType::Blob).map_err(|err| Error::BadRequest(err.into()))
    }
    ColumnDataType::Integer => {
      Cursor::parse(cursor, CursorType::Integer).map_err(|err| Error::BadRequest(err.into()))
    }
    _ => Err(Error::BadRequest("Invalid cursor column type".into())),
  };
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use trailbase_sqlvalue::Blob;

  use super::*;
  use crate::{admin::rows::list_rows::Pagination, app_state::*, listing::WhereClause};

  #[tokio::test]
  async fn test_fetch_rows() {
    let pattern = serde_json::from_str(
      r#"{
          "type": "object",
          "properties": {
            "name": {
              "type": "string"
            }
          }
        }"#,
    )
    .unwrap();

    let json_schema_registry = Some(
      trailbase_schema::registry::build_json_schema_registry(vec![("foo".to_string(), pattern)])
        .unwrap(),
    );

    let state = test_state(Some(TestStateOptions {
      json_schema_registry,
      ..Default::default()
    }))
    .await
    .unwrap();
    let conn = state.conn();

    conn
      .execute_batch(
        r#"CREATE TABLE test_table (
            text    TEXT NOT NULL,
            number  INTEGER,
            blob    BLOB NOT NULL,
            json    TEXT CHECK(jsonschema('foo', json))
          ) STRICT;

          INSERT INTO test_table (text, number, blob, json) VALUES ('test', 5, X'FACE', '{"name": "alice"}');
          "#,
      )
      .await
      .unwrap();

    let cnt: i64 = conn
      .read_query_row_f("SELECT COUNT(*) FROM test_table", (), |row| row.get(0))
      .await
      .unwrap()
      .unwrap();
    assert_eq!(cnt, 1);

    state.rebuild_connection_metadata().await.unwrap();

    let (rows, cols) = fetch_rows(
      conn,
      &QualifiedName {
        name: "test_table".to_string(),
        database_schema: Some("main".to_string()),
      },
      WhereClause {
        clause: "TRUE".to_string(),
        params: vec![],
      },
      &None,
      Pagination {
        cursor_column: None,
        cursor: None,
        offset: None,
        limit: 100,
      },
    )
    .await
    .unwrap();

    assert_eq!(cols.len(), 4);

    let row = rows.get(0).unwrap();

    assert!(matches!(row.get(0).unwrap(), SqlValue::Text(_)));
    assert!(matches!(row.get(1).unwrap(), SqlValue::Integer(_)));

    // CHECK that blob gets serialized as padded, url-safe base64.
    let SqlValue::Blob(Blob::Base64UrlSafe(b64)) = row.get(2).unwrap() else {
      panic!("Not a string: {:?}", row);
    };
    let blob = BASE64_URL_SAFE.decode(b64).unwrap();
    assert_eq!(format!("{:X?}", blob), "[FA, CE]");

    // CHECK that fetch_rows doesn't expand nested JSON.
    let SqlValue::Text(json) = row.get(3).unwrap() else {
      panic!("Not a string: {:?}", row);
    };
    assert_eq!(json, r#"{"name": "alice"}"#);
  }
}
