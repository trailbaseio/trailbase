use axum::extract::{Json, Path, RawQuery, State};
use log::*;
use serde::Serialize;
use std::borrow::Cow;
use std::sync::Arc;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::listing::{
  build_filter_where_clause, limit_or_default, parse_query, Order, WhereClause,
};
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::schema::Column;
use crate::table_metadata::TableOrViewMetadata;

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListRowsResponse {
  pub total_row_count: i64,
  pub cursor: Option<String>,

  pub columns: Vec<Column>,

  // NOTE: use `Object` rather than object to include primitive types.
  #[ts(type = "Object[][]")]
  pub rows: Vec<Vec<serde_json::Value>>,
}

pub async fn list_rows_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListRowsResponse>, Error> {
  let (filter_params, cursor, offset, limit, order) = match parse_query(raw_url_query) {
    Some(q) => (Some(q.params), q.cursor, q.offset, q.limit, q.order),
    None => (None, None, None, None, None),
  };

  let (virtual_table, table_or_view_metadata): (bool, Arc<dyn TableOrViewMetadata + Sync + Send>) = {
    if let Some(table_metadata) = state.table_metadata().get(&table_name) {
      (table_metadata.schema.virtual_table, table_metadata)
    } else if let Some(view_metadata) = state.table_metadata().get_view(&table_name) {
      (false, view_metadata)
    } else {
      return Err(Error::Precondition(format!(
        "Table or view '{table_name}' not found"
      )));
    }
  };

  // Where clause contains column filters and cursor depending on what's present in the url query
  // string.
  let filter_where_clause = build_filter_where_clause(&*table_or_view_metadata, filter_params)?;

  let total_row_count = {
    let where_clause = &filter_where_clause.clause;
    let count_query = format!("SELECT COUNT(*) FROM '{table_name}' WHERE {where_clause}");
    let row = crate::util::query_one_row(
      state.conn(),
      &count_query,
      filter_where_clause.params.clone(),
    )
    .await?;

    row.get::<i64>(0)?
  };

  let cursor_column = table_or_view_metadata.record_pk_column();
  let (rows, columns) = fetch_rows(
    state.conn(),
    &table_name,
    filter_where_clause,
    order,
    Pagination {
      cursor_column: cursor_column.map(|(_idx, c)| c),
      cursor,
      offset,
      limit: limit_or_default(limit),
    },
  )
  .await?;

  let next_cursor = cursor_column.and_then(|(col_idx, _col)| {
    let row = rows.last()?;
    assert!(row.len() > col_idx);
    match &row[col_idx] {
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
    columns: match virtual_table {
      true => columns.unwrap_or_else(Vec::new),
      false => table_or_view_metadata.columns().unwrap_or_else(|| {
        debug!("Falling back to inferred cols for view: '{table_name}'");
        columns.unwrap_or_else(Vec::new)
      }),
    },
    rows,
  }));
}

struct Pagination<'a> {
  cursor_column: Option<&'a Column>,
  cursor: Option<[u8; 16]>,
  offset: Option<usize>,
  limit: usize,
}

async fn fetch_rows(
  conn: &trailbase_sqlite::Connection,
  table_or_view_name: &str,
  filter_where_clause: WhereClause,
  order: Option<Vec<(String, Order)>>,
  pagination: Pagination<'_>,
) -> Result<(Vec<Vec<serde_json::Value>>, Option<Vec<Column>>), Error> {
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

  if let Some(cursor) = pagination.cursor {
    params.push((
      Cow::Borrowed(":cursor"),
      trailbase_sqlite::Value::Blob(cursor.to_vec()),
    ));
    clause = format!("{clause} AND _row_.id < :cursor",);
  }

  let order_clause = match order {
    Some(order) => order
      .iter()
      .map(|(col, ord)| {
        format!(
          "_row_.{col} {}",
          match ord {
            Order::Descending => "DESC",
            Order::Ascending => "ASC",
          }
        )
      })
      .collect::<Vec<_>>()
      .join(", "),
    None => match pagination.cursor_column {
      Some(col) => format!("{col_name} DESC", col_name = col.name),
      None => "NULL".to_string(),
    },
  };

  let query = format!(
    r#"
      SELECT _row_.*
      FROM
        (SELECT * FROM {table_or_view_name}) as _row_
      WHERE
        {clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
      OFFSET :offset
    "#,
  );

  let result_rows = conn.query(&query, params).await.map_err(|err| {
    #[cfg(debug_assertions)]
    error!("QUERY: {query}\n\t=> {err}");

    return err;
  })?;

  return Ok(rows_to_json_arrays(result_rows, 1024)?);
}
