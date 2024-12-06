use axum::body::Body;
use axum::http::{header::HeaderMap, Request};
use axum::response::Response;
use axum_client_ip::InsecureClientIp;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tracing::field::Field;
use tracing::span::{Attributes, Id, Record, Span};
use tracing_subscriber::layer::{Context, Layer};

use crate::AppState;

// Memo to my future self.
//
// Tracing is quite sweet but also utterly decoupled. There are several moving parts.
//
//  * There's a tracing layer installed into the axum/tower server, which declares *what*
//    information goes into traces, i.e. which fields go into spans and events. An event (e.g.
//    on-request, on-response) can comprise a list of spans.
//  * There's a central tracing_subscriber::registry(), where one can register subscribers like an
//    stderr, file, or sqlite logger, that define how traces are being processed.
//  * Finally, we have a task to receive logs from our sqlite tracing subscribers and write them to
//    the database.
//  * We have a period task to wipe logs past their retention.
//
#[repr(i32)]
#[derive(Debug, Clone, Deserialize, Serialize)]
enum LogType {
  Undefined = 0,
  AdminRequest = 1,
  HttpRequest = 2,
  RecordApiRequest = 3,
}

/// DB schema representation.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Log {
  pub id: Option<[u8; 16]>,
  pub created: Option<f64>,
  pub r#type: i32,

  pub level: i32,
  pub status: u16,
  pub method: String,
  pub url: String,

  // milliseconds
  pub latency: f64,
  pub client_ip: String,
  pub referer: String,
  pub user_agent: String,

  pub data: Option<serde_json::Value>,
}

pub(super) fn sqlite_logger_make_span(request: &Request<Body>) -> Span {
  let headers = request.headers();
  let host = get_header(headers, "host").unwrap_or("");
  let user_agent = get_header(headers, "user-agent").unwrap_or("");
  let referer = get_header(headers, "referer").unwrap_or("");
  let client_ip = InsecureClientIp::from(headers, request.extensions()).map(|ip| ip.0.to_string());

  // NOTE: "%" means print using fmt::Display, and "?" means fmt::Debug.
  let span = tracing::info_span!(
      "request",
      method = %request.method(),
      uri = %request.uri(),
      version = ?request.version(),
      host,
      client_ip = client_ip.as_ref().map_or("", |s| s.as_str()),
      user_agent,
      referer,
  );

  return span;
}

pub(super) fn sqlite_logger_on_request(_req: &Request<Body>, _span: &Span) {
  // We're deliberately not creating a request event, since we're already inserting all the
  // request related information into the span
}

pub(super) fn sqlite_logger_on_response(
  response: &Response<Body>,
  latency: Duration,
  _span: &Span,
) {
  let length = get_header(response.headers(), "content-length");

  tracing::info!(
      name: "response",
      latency_ms = as_millis_f64(&latency),
      status = response.status().as_u16(),
      length = length.and_then(|l| l.parse::<i64>().ok()),
  );
}

pub struct SqliteLogLayer {
  sender: tokio::sync::mpsc::UnboundedSender<Box<LogFieldStorage>>,
}

impl SqliteLogLayer {
  pub fn new(state: &AppState) -> Self {
    // NOTE: We're boxing the channel contents to lower the growth rate of back-stopped unbound
    // channels. The underlying container doesn't seem to every shrink :/.
    //
    // TODO: should we use a bounded receiver to create back-pressure?
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();

    let conn = state.logs_conn().clone();
    let rt = tokio::runtime::Handle::current();
    rt.spawn(async move {
      const LIMIT: usize = 128;
      let mut buffer = Vec::<Box<LogFieldStorage>>::with_capacity(LIMIT);

      while receiver.recv_many(&mut buffer, LIMIT).await > 0 {
        let logs = std::mem::take(&mut buffer);

        let result = conn
          .call(move |conn| {
            if logs.len() > 1 {
              let tx = conn.transaction()?;
              for log in logs {
                SqliteLogLayer::insert_log(&tx, log)?;
              }
              tx.commit()?;
            } else {
              for log in logs {
                Self::insert_log(conn, log)?
              }
            }

            Ok(())
          })
          .await;

        if let Err(err) = result {
          warn!("Failed to send logs: {err}");
        }
      }
    });

    return SqliteLogLayer { sender };
  }

  // The writer runs in a separate Task in the background and receives Logs via a channel, which it
  // then writes to Sqlite.
  #[inline]
  fn write_log(&self, log: LogFieldStorage) {
    self.sender.send(Box::new(log)).expect(BUG_TEXT);
  }

  #[inline]
  fn insert_log(
    conn: &rusqlite::Connection,
    log: Box<LogFieldStorage>,
  ) -> Result<(), rusqlite::Error> {
    lazy_static::lazy_static! {
      static ref QUERY: String = indoc::formatdoc! {"
        INSERT INTO
          _logs (type, level, status, method, url, latency, client_ip, referer, user_agent)
        VALUES
          ($1, $2, $3, $4, $5, $6, $7, $8, $9)
      "};
    }

    let mut stmt = conn.prepare_cached(&QUERY)?;
    stmt.execute((
      // FIXME: we're not writing the JSON data.
      // FIXME: type-field is hard-coded. Should be: admin, records, auth, other request
      LogType::HttpRequest as i32,
      log.level,
      log.status,
      log.method,
      log.uri,
      log.latency_ms,
      log.client_ip,
      log.referer,
      log.user_agent,
    ))?;

    return Ok(());
  }
}

impl<S> Layer<S> for SqliteLogLayer
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
    let span = ctx.span(id).expect(BUG_TEXT);

    let mut storage = LogFieldStorage::default();
    attrs.record(&mut LogJsonVisitor(&mut storage));
    span.extensions_mut().insert(storage);
  }

  fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
    let span = ctx.span(id).expect(BUG_TEXT);

    let mut extensions = span.extensions_mut();
    if let Some(storage) = extensions.get_mut::<LogFieldStorage>() {
      values.record(&mut LogJsonVisitor(storage));
    } else {
      info!("logs already consumed");
    }
  }

  fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
    let span = ctx.event_span(event).expect(BUG_TEXT);

    let mut extensions = span.extensions_mut();
    if let Some(mut storage) = extensions.remove::<LogFieldStorage>() {
      event.record(&mut LogJsonVisitor(&mut storage));

      storage.level = level_to_int(event.metadata().level());

      self.write_log(storage);
    }
  }
}

#[derive(Debug, Default, Clone)]
struct LogFieldStorage {
  // Request fields/properties.
  method: String,
  uri: String,
  client_ip: String,
  host: String,
  referer: String,
  user_agent: String,
  version: String,

  // Log level.
  level: i64,

  // Response fields/properties
  status: u64,
  latency_ms: f64,
  length: i64,

  // All other fields.
  fields: serde_json::Map<String, serde_json::Value>,
}

struct LogJsonVisitor<'a>(&'a mut LogFieldStorage);

impl tracing::field::Visit for LogJsonVisitor<'_> {
  fn record_f64(&mut self, field: &Field, double: f64) {
    match field.name() {
      "latency_ms" => self.0.latency_ms = double,
      name => {
        self.0.fields.insert(name.into(), double.into());
      }
    };
  }

  fn record_i64(&mut self, field: &Field, int: i64) {
    match field.name() {
      "length" => self.0.length = int,
      name => {
        self.0.fields.insert(name.into(), int.into());
      }
    };
  }

  fn record_u64(&mut self, field: &Field, uint: u64) {
    match field.name() {
      "status" => self.0.status = uint,
      name => {
        self.0.fields.insert(name.into(), uint.into());
      }
    };
  }

  fn record_bool(&mut self, field: &Field, b: bool) {
    self.0.fields.insert(field.name().into(), b.into());
  }

  fn record_str(&mut self, field: &Field, s: &str) {
    match field.name() {
      "client_ip" => self.0.client_ip = s.to_string(),
      "host" => self.0.host = s.to_string(),
      "referer" => self.0.referer = s.to_string(),
      "user_agent" => self.0.user_agent = s.to_string(),
      name => {
        self.0.fields.insert(name.into(), s.into());
      }
    };
  }

  fn record_debug(&mut self, field: &Field, dbg: &dyn std::fmt::Debug) {
    let v = format!("{:?}", dbg);
    match field.name() {
      "method" => self.0.method = v,
      "uri" => self.0.uri = v,
      "version" => self.0.version = v,
      name => {
        self.0.fields.insert(name.into(), v.into());
      }
    };
  }

  fn record_error(&mut self, field: &Field, err: &(dyn std::error::Error + 'static)) {
    self
      .0
      .fields
      .insert(field.name().into(), json!(err.to_string()));
  }
}

#[inline]
fn as_millis_f64(d: &Duration) -> f64 {
  const NANOS_PER_MILLI: f64 = 1_000_000.0;
  const MILLIS_PER_SEC: u64 = 1_000;

  return (d.as_secs() as f64) * (MILLIS_PER_SEC as f64)
    + (d.subsec_nanos() as f64) / (NANOS_PER_MILLI);
}

#[inline]
fn get_header<'a>(headers: &'a HeaderMap, header_name: &'static str) -> Option<&'a str> {
  headers
    .get(header_name)
    .and_then(|header_value| header_value.to_str().ok())
}

#[inline]
fn level_to_int(level: &tracing::Level) -> i64 {
  match *level {
    tracing::Level::TRACE => 4,
    tracing::Level::DEBUG => 3,
    tracing::Level::INFO => 2,
    tracing::Level::WARN => 1,
    tracing::Level::ERROR => 0,
  }
}

const BUG_TEXT: &str = "Span not found, this is a bug";
