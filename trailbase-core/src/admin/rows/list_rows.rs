use axum::extract::{Json, Path, RawQuery, State};
use log::*;
use serde::Serialize;
use std::borrow::Cow;
use std::sync::Arc;
use trailbase_qs::{Cursor, CursorType, Order, OrderPrecedent, Query};
use trailbase_schema::QualifiedName;
use trailbase_schema::sqlite::{Column, ColumnDataType};
use trailbase_sqlite::rows::rows_to_json_arrays;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::listing::{WhereClause, build_filter_where_clause, cursor_to_value, limit_or_default};
use crate::schema_metadata::{TableMetadata, TableOrViewMetadata};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListRowsResponse {
  pub total_row_count: i64,
  pub cursor: Option<String>,

  pub columns: Vec<Column>,

  // NOTE: that we're not expanding nested JSON and are encoding Blob to
  // base64, this is thus a just a JSON subset. See test below.
  #[ts(type = "(string | number | boolean | null)[][]")]
  pub rows: Vec<Vec<serde_json::Value>>,
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
  let (schema_metadata, table_or_view_metadata): (
    Option<Arc<TableMetadata>>,
    Arc<dyn TableOrViewMetadata + Sync + Send>,
  ) = {
    if let Some(schema_metadata) = state.schema_metadata().get_table(&table_name) {
      (Some(schema_metadata.clone()), schema_metadata)
    } else if let Some(view_metadata) = state.schema_metadata().get_view(&table_name) {
      (None, view_metadata)
    } else {
      return Err(Error::Precondition(format!(
        "Table or view '{table_name:?}' not found"
      )));
    }
  };
  let qualified_name = table_or_view_metadata.qualified_name();

  // Where clause contains column filters and cursor depending on what's present in the url query
  // string.
  let filter_where_clause = if let Some(columns) = table_or_view_metadata.columns() {
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
    state
      .conn()
      .read_query_row_f(count_query, filter_where_clause.params.clone(), |row| {
        row.get(0)
      })
      .await?
      .unwrap_or(-1)
  };

  let cursor_column = table_or_view_metadata.record_pk_column();
  let cursor = match (cursor, cursor_column) {
    (Some(cursor), Some((_idx, c))) => Some(parse_cursor(&cursor, c)?),
    _ => None,
  };
  let (rows, columns) = fetch_rows(
    state.conn(),
    qualified_name,
    filter_where_clause,
    &order,
    Pagination {
      cursor_column: cursor_column.map(|(_idx, c)| c),
      cursor,
      offset,
      limit: limit_or_default(limit).map_err(|err| Error::BadRequest(err.into()))?,
    },
  )
  .await?;

  let next_cursor = cursor_column.and_then(|(col_idx, _col)| {
    let row = rows.last()?;
    assert!(row.len() > col_idx);
    match &row[col_idx] {
      serde_json::Value::Number(n) if n.is_i64() => Some(n.to_string()),
      serde_json::Value::String(id) => {
        // Should be a base64 encoded [u8; 16] id.
        Some(id.clone())
      }
      _ => None,
    }
  });

  return Ok(Json(ListRowsResponse {
    total_row_count,
    cursor: next_cursor,
    // NOTE: in the view case we don't have a good way of extracting the columns from the "CREATE
    // VIEW" query so we fall back to columns constructed from the returned data.
    columns: match schema_metadata {
      Some(ref metadata) if metadata.schema.virtual_table => {
        // Virtual TABLE case.
        columns
      }
      Some(ref metadata) => {
        // Non-virtual TABLE case.
        metadata.schema.columns.clone()
      }
      _ => {
        // VIEW-case
        if let Some(columns) = table_or_view_metadata.columns() {
          columns.to_vec()
        } else {
          debug!("Falling back to inferred cols for view: {table_name:?}");
          columns
        }
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
) -> Result<(Vec<Vec<serde_json::Value>>, Vec<Column>), Error> {
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
    params.push((Cow::Borrowed(":cursor"), cursor_to_value(cursor)));
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
      SELECT _ROW_.*
      FROM
        (SELECT * FROM {table}) as _ROW_
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
    rows_to_json_arrays(&result_rows)?,
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

  use super::*;
  use crate::{admin::rows::list_rows::Pagination, app_state::*, listing::WhereClause};

  #[tokio::test]
  async fn test_fetch_rows() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

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

    trailbase_schema::registry::set_user_schema("foo", Some(pattern)).unwrap();

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

    state.schema_metadata().invalidate_all().await.unwrap();

    let (data, cols) = fetch_rows(
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

    let row = data.get(0).unwrap();

    assert!(row.get(0).unwrap().is_string());
    assert!(row.get(1).unwrap().is_i64());

    // CHECK that blob gets serialized as padded, url-safe base64.
    let serde_json::Value::String(b64) = row.get(2).unwrap() else {
      panic!("Not a string: {:?}", row);
    };
    let blob = BASE64_URL_SAFE.decode(b64).unwrap();
    assert_eq!(format!("{:X?}", blob), "[FA, CE]");

    // CHECK that fetch_rows doesn't expand nested JSON.
    let serde_json::Value::String(json) = row.get(3).unwrap() else {
      panic!("Not a string: {:?}", row);
    };
    assert_eq!(json, r#"{"name": "alice"}"#);
  }
}
