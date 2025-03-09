use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use axum_client_ip::InsecureClientIp;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::time::Duration;
use tracing::field::Field;
use tracing::span::{Attributes, Id, Record, Span};
use tracing::Level;
use tracing_subscriber::layer::{Context, Layer};

use crate::constants::{ADMIN_API_PATH, RECORD_API_PATH};
use crate::util::get_header;
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
#[repr(i64)]
#[derive(Debug, Clone, Deserialize, Serialize)]
enum LogType {
  Undefined = 0,
  AdminRequest = 1,
  HttpRequest = 2,
  RecordApiRequest = 3,
}

const LEVEL: Level = Level::INFO;
const NAME: &str = "TB::sqlog";

pub(super) fn sqlite_logger_make_span(request: &Request<Body>) -> Span {
  let headers = request.headers();

  let host = get_header(headers, "host").unwrap_or("");
  let user_agent = get_header(headers, "user-agent").unwrap_or("");
  let referer = get_header(headers, "referer").unwrap_or("");
  let client_ip = InsecureClientIp::from(headers, request.extensions())
    .map(|ip| ip.0.to_string())
    .ok();

  let uri = request.uri();
  let request_type = {
    lazy_static::lazy_static! {
      static ref ADMIN: String = format!("/{ADMIN_API_PATH}");
      static ref RECORD : String = format!("/{RECORD_API_PATH}");
    }

    let path = uri.path();
    if path.starts_with(ADMIN.as_str()) {
      LogType::AdminRequest
    } else if path.starts_with(RECORD.as_str()) {
      LogType::RecordApiRequest
    } else {
      LogType::HttpRequest
    }
  } as i32;

  // NOTE: "%" means print using fmt::Display, and "?" means fmt::Debug.
  return tracing::span!(
      LEVEL,
      NAME,
      request_type,
      method = %request.method(),
      uri = %request.uri(),
      version = ?request.version(),
      host,
      client_ip,
      user_agent,
      referer,
      latency_ms = tracing::field::Empty,
      status = tracing::field::Empty,
      length = tracing::field::Empty,
  );
}

pub(super) fn sqlite_logger_on_request(_req: &Request<Body>, _span: &Span) {
  // We don't need to record anything extra, since we already unpacked the request during span
  // creation above.
}

pub(super) fn sqlite_logger_on_response(response: &Response<Body>, latency: Duration, span: &Span) {
  let length = get_header(response.headers(), "content-length");
  span.record("latency_ms", as_millis_f64(&latency));
  span.record("status", response.status().as_u16());
  span.record("length", length.and_then(|l| l.parse::<i64>().ok()));

  // Log an event that can actually be seen, e.g. when a stderr logger is installed.
  tracing::event!(LEVEL, "response sent");
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
    if let Err(err) = self.sender.send(Box::new(log)) {
      panic!("Sending logs failed: {err}");
    }
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
      // TODO: we're yet not writing extra JSON data to the data field.
      log.r#type,
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
  /// When a new "__tbreq" span is created, attach field storage.
  fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
    let span = ctx.span(id).expect("span must exist in on_new_span");

    let mut storage = LogFieldStorage::default();
    attrs.record(&mut LogJsonVisitor(&mut storage));
    span.extensions_mut().insert(storage);
  }

  // Then the (request->response) span "__tbreq" is closed, write out the logs.
  fn on_close(&self, id: Id, ctx: Context<'_, S>) {
    let Some(span) = ctx.span(&id) else {
      return;
    };
    let metadata = span.metadata();
    if metadata.name() != NAME {
      // TODO: If we're in a child of the request-response-span, we should recursively merge field
      // storages into their parents in on_close. This becomes relevant if we want to allow
      // user-defined spans and custom fields logged to Log::data. Alternatively, we could traverse
      // up to the spans to fill the root storage in on_record & on_event.
      return;
    }

    let mut extensions = span.extensions_mut();
    if let Some(mut storage) = extensions.remove::<LogFieldStorage>() {
      storage.level = level_to_int(metadata.level());

      self.write_log(storage);
    } else {
      error!("span already closed/consumed?!");
    }
  }

  // When span.record() is called, add to field storage.
  fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
    let Some(span) = ctx.span(id) else {
      return;
    };

    if !values.is_empty() {
      let mut extensions = span.extensions_mut();
      if let Some(storage) = extensions.get_mut::<LogFieldStorage>() {
        values.record(&mut LogJsonVisitor(storage));
      }
    }
  }

  // Add events to field storage.
  fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
    let Some(span) = ctx.event_span(event) else {
      return;
    };

    let mut extensions = span.extensions_mut();
    if let Some(storage) = extensions.get_mut::<LogFieldStorage>() {
      event.record(&mut LogJsonVisitor(storage));
    }
  }
}

#[derive(Debug, Default, Clone)]
struct LogFieldStorage {
  // Request fields/properties.
  r#type: i64,
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
      "request_type" => self.0.r#type = int,
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
    match field.name() {
      "method" => self.0.method = format!("{:?}", dbg),
      "uri" => self.0.uri = format!("{:?}", dbg),
      "version" => self.0.version = format!("{:?}", dbg),
      _name => {
        // Skip "messages" and other fields, we only log structured data.
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
fn level_to_int(level: &tracing::Level) -> i64 {
  match *level {
    tracing::Level::TRACE => 4,
    tracing::Level::DEBUG => 3,
    tracing::Level::INFO => 2,
    tracing::Level::WARN => 1,
    tracing::Level::ERROR => 0,
  }
}
