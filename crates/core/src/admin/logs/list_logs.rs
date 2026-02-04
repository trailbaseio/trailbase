use axum::{
  Json,
  extract::{RawQuery, State},
};
use lazy_static::lazy_static;
use log::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use trailbase_extension::geoip::{City, DatabaseType};
use trailbase_qs::{Order, OrderPrecedent, Query};
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::constants::{LOGS_TABLE, LOGS_TABLE_ID_COLUMN};
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::schema_metadata::{TableMetadata, lookup_and_parse_table_schema};

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListLogsResponse {
  total_row_count: i64,
  cursor: Option<String>,
  entries: Vec<LogJson>,
}

pub async fn list_logs_handler(
  State(state): State<AppState>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListLogsResponse>, Error> {
  let conn = state.logs_conn();

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

  let filter_where_clause = {
    // NOTE: We cannot get `ConnectionMetadata` via the connection_manager() here because logs are
    // in a different DB.
    let table = lookup_and_parse_table_schema(conn, LOGS_TABLE, None).await?;
    let table_metadata = TableMetadata::new(
      &trailbase_extension::jsonschema::JsonSchemaRegistry::from_schemas(vec![]),
      table.clone(),
      &[table],
    )?;

    build_filter_where_clause(TABLE_ALIAS, &table_metadata.schema.columns, filter_params)?
  };

  let total_row_count: i64 = conn
    .read_query_row_f(
      format!(
        "SELECT COUNT(*) FROM {LOGS_TABLE} AS {TABLE_ALIAS} WHERE {where_clause}",
        where_clause = filter_where_clause.clause
      ),
      filter_where_clause.params.clone(),
      |row| row.get(0),
    )
    .await?
    .unwrap_or(-1);

  lazy_static! {
    static ref DEFAULT_ORDERING: Order = Order {
      columns: vec![(LOGS_TABLE_ID_COLUMN.to_string(), OrderPrecedent::Descending)],
    };
  }

  let supports_cursor = order.is_none();
  let cursor = if supports_cursor && let Some(cursor) = cursor {
    Some(
      cursor
        .parse::<i64>()
        .map_err(|err| Error::BadRequest(err.into()))?,
    )
  } else {
    None
  };

  let geoip_db_type = trailbase_extension::geoip::database_type();
  let mut logs = fetch_logs(
    conn,
    geoip_db_type.clone(),
    filter_where_clause.clone(),
    cursor,
    offset,
    order.as_ref().unwrap_or_else(|| &DEFAULT_ORDERING),
    limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))?,
  )
  .await?;

  if state.demo_mode() {
    for entry in &mut logs {
      entry.redact();
    }
  }

  let next_cursor = if supports_cursor {
    logs.last().map(|log| {
      #[cfg(debug_assertions)]
      if let Some(old_cursor) = cursor {
        assert!(old_cursor > log.id);
      }

      return log.id.to_string();
    })
  } else {
    None
  };

  return Ok(Json(ListLogsResponse {
    total_row_count,
    cursor: next_cursor,
    entries: logs
      .into_iter()
      .map(|log| log.into())
      .collect::<Vec<LogJson>>(),
  }));
}

async fn fetch_logs(
  conn: &trailbase_sqlite::Connection,
  geoip_db_type: Option<DatabaseType>,
  filter_where_clause: WhereClause,
  cursor: Option<i64>,
  offset: Option<usize>,
  order: &Order,
  limit: usize,
) -> Result<Vec<LogEntry>, Error> {
  let mut params = filter_where_clause.params;
  let mut where_clause = filter_where_clause.clause;
  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(limit as i64),
  ));

  params.push((
    Cow::Borrowed(":offset"),
    trailbase_sqlite::Value::Integer(offset.map_or(0, |o| o.try_into().unwrap_or(0))),
  ));

  if let Some(cursor) = cursor {
    params.push((
      Cow::Borrowed(":cursor"),
      trailbase_sqlite::Value::Integer(cursor),
    ));
    where_clause = format!("{where_clause} AND {TABLE_ALIAS}.id < :cursor",);
  }

  let order_clause = order
    .columns
    .iter()
    .map(|(col, ord)| {
      format!(
        "{TABLE_ALIAS}.{col} {}",
        match ord {
          OrderPrecedent::Descending => "DESC",
          OrderPrecedent::Ascending => "ASC",
        }
      )
    })
    .collect::<Vec<_>>()
    .join(", ");

  let sql_query = format!(
    r#"
      SELECT {TABLE_ALIAS}.*, {geoip}
      FROM
        (SELECT * FROM {LOGS_TABLE}) AS {TABLE_ALIAS}
      WHERE
        {where_clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
      OFFSET :offset
    "#,
    geoip = match geoip_db_type {
      Some(DatabaseType::GeoLite2Country) =>
        format!("geoip_country({TABLE_ALIAS}.client_ip) AS client_geoip_cc"),
      Some(DatabaseType::GeoLite2City) =>
        format!("geoip_city_json({TABLE_ALIAS}.client_ip) AS client_geoip_city"),
      _ => "''".to_string(),
    },
  );

  return Ok(
    conn
      .read_query_values::<LogEntry>(sql_query, params)
      .await?,
  );
}

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct GeoipCity {
  country_code: Option<String>,
  name: Option<String>,
}

#[derive(Debug, Serialize, TS)]
pub struct LogJson {
  pub id: i64,
  pub created: f64,

  pub status: u16,
  pub method: String,
  pub url: String,

  pub latency_ms: f64,
  pub client_ip: String,
  /// Optional two-letter country code.
  pub client_geoip_cc: Option<String>,
  pub client_geoip_city: Option<GeoipCity>,

  pub referer: String,
  pub user_agent: String,
  pub user_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LogEntry {
  id: i64,
  created: f64,

  status: u16,
  method: String,
  url: String,

  // Latency in fractional milliseconds.
  latency: f64,
  client_ip: String,
  /// Optional two-letter country code.
  client_geoip_cc: Option<String>,
  /// Optional city JSON.
  client_geoip_city: Option<String>,

  referer: String,
  user_agent: String,
  user_id: Option<[u8; 16]>,
  // data: Option<Vec<u8>>,
}

impl LogEntry {
  fn redact(&mut self) {
    fn replace_if_set(field: &mut String) {
      if !field.is_empty() {
        *field = "<demo>".to_string()
      }
    }

    replace_if_set(&mut self.client_ip);
    replace_if_set(&mut self.referer);
    replace_if_set(&mut self.user_agent);
  }
}

impl From<LogEntry> for LogJson {
  fn from(value: LogEntry) -> Self {
    return LogJson {
      id: value.id,
      created: value.created,
      status: value.status,
      method: value.method,
      url: value.url,
      latency_ms: value.latency,
      client_ip: value.client_ip,
      client_geoip_cc: value.client_geoip_cc,
      client_geoip_city: value.client_geoip_city.and_then(|city| {
        // NOTE: We could propably parse the JSON as `GeoipCity` right away :shrug:.
        return serde_json::from_str::<City>(&city)
          .map_err(|err| {
            warn!("Failed to parse geoip city json: {err}");
            return err;
          })
          .map(|city| GeoipCity {
            country_code: city.country_code,
            name: city.name,
          })
          .ok();
      }),
      referer: value.referer,
      user_agent: value.user_agent,
      user_id: value.user_id.map(|blob| Uuid::from_bytes(blob).to_string()),
    };
  }
}

const TABLE_ALIAS: &str = "log";
