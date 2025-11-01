use axum::{
  Json,
  extract::{RawQuery, State},
};
use chrono::{DateTime, Duration, Utc};
use lazy_static::lazy_static;
use log::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;
use trailbase_extension::geoip::{City, DatabaseType};
use trailbase_qs::{Order, OrderPrecedent, Query};
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::constants::{LOGS_RETENTION_DEFAULT, LOGS_TABLE_ID_COLUMN};
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::schema_metadata::{TableMetadata, lookup_and_parse_table_schema};

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct GeoipCity {
  country_code: Option<String>,
  name: Option<String>,
}

impl From<City> for GeoipCity {
  fn from(city: City) -> Self {
    return Self {
      country_code: city.country_code,
      name: city.name,
    };
  }
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
        return serde_json::from_str::<City>(&city)
          .map_err(|err| {
            log::warn!("Failed to parse geoip city json: {err}");
            return err;
          })
          .map(|city| city.into())
          .ok();
      }),
      referer: value.referer,
      user_agent: value.user_agent,
      user_id: value.user_id.map(|blob| Uuid::from_bytes(blob).to_string()),
    };
  }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListLogsResponse {
  total_row_count: i64,
  cursor: Option<String>,
  entries: Vec<LogJson>,

  stats: Option<Stats>,
}

pub async fn list_logs_handler(
  State(state): State<AppState>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListLogsResponse>, Error> {
  let conn = state.logs_conn();

  // TODO: we should probably return an error if the query parsing fails rather than quietly
  // falling back to defaults.
  let Query {
    limit,
    cursor,
    order,
    filter: filter_params,
    ..
  } = raw_url_query
    .as_ref()
    .map_or_else(|| Ok(Query::default()), |query| Query::parse(query))
    .map_err(|err| {
      return Error::BadRequest(format!("Invalid query '{err}': {raw_url_query:?}").into());
    })?;

  // NOTE: We cannot use state.connection_metadata() here, which is managed via a different database
  // *and* connection.
  let table = lookup_and_parse_table_schema(conn, LOGS_TABLE_NAME, None).await?;

  let table_metadata = TableMetadata::new(
    &trailbase_extension::jsonschema::JsonSchemaRegistry::default(),
    table.clone(),
    &[table],
  );
  let filter_where_clause =
    build_filter_where_clause("log", &table_metadata.schema.columns, filter_params)?;

  let total_row_count: i64 = conn
    .read_query_row_f(
      format!(
        "SELECT COUNT(*) FROM {LOGS_TABLE_NAME} AS log WHERE {where_clause}",
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

  let cursor = if let Some(cursor) = cursor {
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
    order.as_ref().unwrap_or_else(|| &DEFAULT_ORDERING),
    limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))?,
  )
  .await?;

  if state.demo_mode() {
    for entry in &mut logs {
      entry.redact();
    }
  }

  let first_page = cursor.is_none();
  let stats = if first_page {
    let now = Utc::now();
    let args = FetchAggregateArgs {
      geoip_db_type,
      filter_where_clause: Some(filter_where_clause),
      from: now
        - Duration::seconds(state.access_config(|c| {
          c.server
            .logs_retention_sec
            .unwrap_or_else(|| LOGS_RETENTION_DEFAULT.num_seconds())
        })),
      to: now,
      interval: Duration::seconds(600),
    };

    let stats = fetch_aggregate_stats(conn, &args).await;
    if let Err(ref err) = stats {
      warn!("Failed to fetch stats for {args:?}: {err}");
    }

    stats.ok()
  } else {
    None
  };

  let response = ListLogsResponse {
    total_row_count,
    cursor: logs.last().map(|log| {
      if let Some(old_cursor) = cursor {
        assert!(old_cursor > log.id);
      }

      return log.id.to_string();
    }),
    entries: logs
      .into_iter()
      .map(|log| log.into())
      .collect::<Vec<LogJson>>(),
    stats,
  };

  return Ok(Json(response));
}

async fn fetch_logs(
  conn: &trailbase_sqlite::Connection,
  geoip_db_type: Option<DatabaseType>,
  filter_where_clause: WhereClause,
  cursor: Option<i64>,
  order: &Order,
  limit: usize,
) -> Result<Vec<LogEntry>, Error> {
  let mut params = filter_where_clause.params;
  let mut where_clause = filter_where_clause.clause;
  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(limit as i64),
  ));

  if let Some(cursor) = cursor {
    params.push((
      Cow::Borrowed(":cursor"),
      trailbase_sqlite::Value::Integer(cursor),
    ));
    where_clause = format!("{where_clause} AND log.id < :cursor",);
  }

  let order_clause = order
    .columns
    .iter()
    .map(|(col, ord)| {
      format!(
        "log.{col} {}",
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
      SELECT log.*, {geoip}
      FROM
        (SELECT * FROM {LOGS_TABLE_NAME}) AS log
      WHERE
        {where_clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
    "#,
    geoip = match geoip_db_type {
      Some(DatabaseType::GeoLite2Country) => "geoip_country(log.client_ip) AS client_geoip_cc",
      Some(DatabaseType::GeoLite2City) => "geoip_city_json(log.client_ip) AS client_geoip_city",
      _ => "''",
    },
  );

  return Ok(
    conn
      .read_query_values::<LogEntry>(sql_query, params)
      .await?,
  );
}

#[derive(Debug, Serialize, TS)]
pub struct Stats {
  // List of (timestamp, value).
  rate: Vec<(i64, f64)>,
  // Country codes.
  country_codes: Option<HashMap<String, usize>>,
}

#[derive(Debug)]
struct FetchAggregateArgs {
  geoip_db_type: Option<DatabaseType>,
  filter_where_clause: Option<WhereClause>,
  from: DateTime<Utc>,
  to: DateTime<Utc>,
  interval: Duration,
}

async fn fetch_aggregate_stats(
  conn: &trailbase_sqlite::Connection,
  args: &FetchAggregateArgs,
) -> Result<Stats, Error> {
  let filter_clause = args
    .filter_where_clause
    .as_ref()
    .map(|c| c.clause.clone())
    .unwrap_or_else(|| "TRUE".to_string());

  #[derive(Deserialize)]
  struct AggRow {
    interval_end_ts: i64,
    count: i64,
  }

  // Aggregate rate of all logs in the same :interval_seconds.
  //
  // Note, we're aligning the interval wide grid with the latest `to` timestamp to minimize
  // artifacts when (to - from) / interval is not an integer. This way we only get artifacts in the
  // oldest interval.
  let qps_query = format!(
    r#"
    SELECT
      CAST(ROUND((created - :to_seconds) / :interval_seconds) AS INTEGER) * :interval_seconds + :to_seconds AS interval_end_ts,
      COUNT(*) as count
    FROM
      (SELECT * FROM {LOGS_TABLE_NAME} WHERE created > :from_seconds AND created < :to_seconds AND {filter_clause} ORDER BY id DESC)
    GROUP BY
      interval_end_ts
    ORDER BY
      interval_end_ts ASC
  "#
  );

  use trailbase_sqlite::Value::Integer;
  let from_seconds = args.from.timestamp();
  let interval_seconds = args.interval.num_seconds();
  let mut params = Vec::<(Cow<'_, str>, trailbase_sqlite::Value)>::with_capacity(16);

  params.extend_from_slice(&[
    (
      Cow::Borrowed(":interval_seconds"),
      Integer(interval_seconds),
    ),
    (Cow::Borrowed(":from_seconds"), Integer(from_seconds)),
    (Cow::Borrowed(":to_seconds"), Integer(args.to.timestamp())),
  ]);

  if let Some(filter) = &args.filter_where_clause {
    params.extend(filter.params.clone())
  }

  let rows = conn.read_query_values::<AggRow>(qps_query, params).await?;

  let mut rate: Vec<(i64, f64)> = vec![];
  for r in rows.iter() {
    // The oldest interval may be clipped if "(to-from)/interval" isn't integer. In this case
    // dividide by a shorter interval length to reduce artifacting. Otherwise, the clipped
    // interval would appear to have a lower rater.
    let effective_interval_seconds = std::cmp::min(
      interval_seconds,
      r.interval_end_ts - (from_seconds - interval_seconds),
    ) as f64;

    rate.push((
      // Use interval midpoint as timestamp.
      r.interval_end_ts - interval_seconds / 2,
      // Compute rate from event count in interval.
      (r.count as f64) / effective_interval_seconds,
    ));
  }

  if args.geoip_db_type == Some(DatabaseType::GeoLite2Country)
    || args.geoip_db_type == Some(DatabaseType::GeoLite2City)
  {
    lazy_static! {
      static ref CC_QUERY: String = format!(
        r#"
          SELECT
            country_code,
            SUM(cnt) as count
          FROM
            (SELECT client_ip, COUNT(*) AS cnt, geoip_country(client_ip) as country_code FROM {LOGS_TABLE_NAME} GROUP BY client_ip)
          GROUP BY
            country_code
        "#
      );
    }

    let rows = conn.read_query_rows(CC_QUERY.as_str(), ()).await?;

    let mut country_codes = HashMap::<String, usize>::new();
    for row in rows.iter() {
      let cc: Option<String> = row.get(0)?;
      let count: i64 = row.get(1)?;

      country_codes.insert(
        cc.unwrap_or_else(|| "unattributed".to_string()),
        count as usize,
      );
    }

    return Ok(Stats {
      rate,
      country_codes: Some(country_codes),
    });
  }

  return Ok(Stats {
    rate,
    country_codes: None,
  });
}

#[cfg(test)]
mod tests {
  use chrono::{DateTime, Duration};

  use super::*;
  use crate::migrations::apply_logs_migrations;

  #[tokio::test]
  async fn test_aggregate_rate_computation() {
    let conn = trailbase_sqlite::Connection::new(
      move || -> anyhow::Result<_> {
        let mut conn_sync =
          crate::connection::connect_rusqlite_without_default_extensions_and_schemas(None).unwrap();
        apply_logs_migrations(&mut conn_sync).unwrap();
        return Ok(conn_sync);
      },
      None,
    )
    .unwrap();

    let interval_seconds = 600;
    let to = DateTime::parse_from_rfc3339("1996-12-22T12:00:00Z").unwrap();
    // An **almost** 24h interval. We make it slightly shorter, so we get some clipping.
    let from = to - Duration::seconds(24 * 3600 - 20);

    {
      // Insert test data.
      let before = (from - Duration::seconds(1)).timestamp();
      let after = (to + Duration::seconds(1)).timestamp();

      let just_inside0 = (from + Duration::seconds(10)).timestamp();
      let just_inside1 = (to - Duration::seconds(10)).timestamp();

      let smack_in_there0 = (from + Duration::seconds(12 * 3600)).timestamp();
      let smack_in_there1 = (from + Duration::seconds(12 * 3600 + 1)).timestamp();

      conn
        .execute_batch(format!(
          r#"
            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({before});
            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({after});

            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({just_inside0});
            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({just_inside1});

            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({smack_in_there0});
            INSERT INTO {LOGS_TABLE_NAME} (created) VALUES({smack_in_there1});
          "#,
        ))
        .await
        .unwrap();
    }

    let args = FetchAggregateArgs {
      geoip_db_type: Some(DatabaseType::Unknown),
      filter_where_clause: None,
      from: from.into(),
      to: to.into(),
      interval: Duration::seconds(interval_seconds),
    };

    let stats = fetch_aggregate_stats(&conn, &args).await.unwrap();

    // Assert that there are 3 data points in the given range and that all of them have a rate of
    // one log in the 600s interval.
    let rates = stats.rate;
    assert_eq!(rates.len(), 3);

    // Assert the oldest, clipped interval has a slightly elevated rate.
    {
      let rate = rates[0];
      assert_eq!(
        DateTime::from_timestamp(rate.0, 0).unwrap(),
        DateTime::parse_from_rfc3339("1996-12-21T11:55:00Z").unwrap()
      );
      assert!(rate.1 > 1.0 / interval_seconds as f64);
    }

    // Assert the middle rate, has two logs, i.e. double the base rate.
    {
      let rate = rates[1];
      assert_eq!(
        DateTime::from_timestamp(rate.0, 0).unwrap(),
        DateTime::parse_from_rfc3339("1996-12-21T23:55:00Z").unwrap()
      );
      assert_eq!(rate.1, 2.0 / interval_seconds as f64);
    }

    // Assert the youngest, most recent interval has the base rate.
    {
      let rate = rates[2];
      assert_eq!(
        DateTime::from_timestamp(rate.0, 0).unwrap(),
        DateTime::parse_from_rfc3339("1996-12-22T11:55:00Z").unwrap()
      );
      assert_eq!(rate.1, 1.0 / interval_seconds as f64);
    }
  }
}

const LOGS_TABLE_NAME: &str = "_logs";
