use axum::body::Body;
use axum::http::{header::HeaderMap, Request};
use axum::response::Response;
use axum_client_ip::InsecureClientIp;
use log::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
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
#[derive(Debug, Clone, Serialize, Deserialize)]
enum LogType {
  Undefined = 0,
  AdminRequest = 1,
  HttpRequest = 2,
  RecordApiRequest = 3,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
  let client_ip = InsecureClientIp::from(headers, request.extensions())
    .map(|ip| ip.0.to_string())
    .unwrap_or_else(|_| "".to_string());
  // let extensions = request.extensions().get::<ConnectInfo<SocketAddr>>();

  let span = tracing::info_span!(
      "request",
      method = %request.method(),
      uri = %request.uri(),
      version = ?request.version(),
      host,
      client_ip,
      user_agent,
      referer,
  );

  return span;
}

pub(super) fn sqlite_logger_on_request(_req: &Request<Body>, _span: &Span) {
  // We're deliberately not creating a request event, since we're already inserting all the
  // request related information into the span
}

fn as_millis_f64(d: &Duration) -> f64 {
  const NANOS_PER_MILLI: f64 = 1_000_000.0;
  const MILLIS_PER_SEC: u64 = 1_000;
  return d.as_secs_f64() * (MILLIS_PER_SEC as f64) + (d.as_nanos() as f64) / (NANOS_PER_MILLI);
}

pub(super) fn sqlite_logger_on_response(
  response: &Response<Body>,
  latency: Duration,
  _span: &Span,
) {
  let length = get_header(response.headers(), "content-length").unwrap_or("-1");

  tracing::info!(
      name: "response",
      latency_ms = as_millis_f64(&latency),
      status = response.status().as_u16(),
      length = length.parse::<i64>().unwrap(),
  );
}

pub struct SqliteLogLayer {
  conn: tokio_rusqlite::Connection,
}

impl SqliteLogLayer {
  pub fn new(state: &AppState) -> Self {
    return SqliteLogLayer {
      conn: state.logs_conn().clone(),
    };
  }

  // The writer runs in a separate Task in the background and receives Logs via a channel, which it
  // then writes to Sqlite.
  //
  // TODO: should we use a bound receiver to create back pressure?
  // TODO: use recv_many() and batch insert.
  fn write_log(&self, log: Log) -> Result<(), tokio_rusqlite::Error> {
    return self.conn.call_and_forget(move |conn| {
      let result = conn.execute(
        r#"
        INSERT INTO
          _logs (type, level, status, method, url, latency, client_ip, referer, user_agent)
        VALUES
          ($1, $2, $3, $4, $5, $6, $7, $8, $9)
      "#,
        rusqlite::params!(
          log.r#type,
          log.level,
          log.status,
          log.method,
          log.url,
          log.latency,
          log.client_ip,
          log.referer,
          log.user_agent
        ),
      );

      if let Err(err) = result {
        warn!("logs writing failed: {err}");
      }
    });
  }
}

impl<S> Layer<S> for SqliteLogLayer
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
    let span = ctx.span(id).unwrap();

    // let mut fields = BTreeMap::new();
    // attrs.record(&mut JsonVisitor(&mut fields));
    // span.extensions_mut().insert(CustomFieldStorage(fields));

    let mut storage = LogFieldStorage::default();
    attrs.record(&mut LogJsonVisitor(&mut storage));
    span.extensions_mut().insert(storage);
  }

  fn on_record(&self, id: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
    // Get a mutable reference to the data we created in new_span
    let span = ctx.span(id).unwrap();
    let mut extensions_mut = span.extensions_mut();

    // And add to using our old friend the visitor!
    // let custom_field_storage = extensions_mut.get_mut::<CustomFieldStorage>().unwrap();
    // values.record(&mut JsonVisitor(&mut custom_field_storage.0));

    let log_field_storage = extensions_mut.get_mut::<LogFieldStorage>().unwrap();
    values.record(&mut LogJsonVisitor(log_field_storage));
  }

  fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
    let mut request_storage: Option<LogFieldStorage> = None;

    let scope = ctx.event_scope(event).unwrap();
    for span in scope.from_root() {
      // TODO: we should be merging here to account for multiple spans. Maybe we should have a json
      // span representation in the data field.
      let extensions = span.extensions();
      if let Some(storage) = extensions.get::<LogFieldStorage>() {
        request_storage = Some(storage.clone());
      }
    }

    // The fields of the event
    // let mut fields = BTreeMap::new();
    // event.record(&mut JsonVisitor(&mut fields));
    // let output = json!({
    //     "target": event.metadata().target(),
    //     "name": event.metadata().name(),
    //     "level": format!("{:?}", event.metadata().level()),
    //     "fields": fields,
    //     "spans": spans,
    // });
    // println!("{}", serde_json::to_string_pretty(&output).unwrap());

    if let Some(mut storage) = request_storage {
      event.record(&mut LogJsonVisitor(&mut storage));

      let log = Log {
        id: None,
        created: None,
        // FIXME: Is it a admin/records/auth,plain http request...?
        // Or should this even be here? Couldn't we just infer client-side by prefix?
        r#type: LogType::HttpRequest as i32,
        level: level_to_int(event.metadata().level()),
        status: storage.status as u16,
        method: storage.method,
        url: storage.uri,
        latency: storage.latency_ms,
        client_ip: storage.client_ip,
        referer: storage.referer,
        user_agent: storage.user_agent,
        data: Some(json!(storage.fields)),
      };

      if let Err(err) = self.write_log(log) {
        warn!("Failed to send to logs to writer: {err}");
      }
    }
  }
}

fn level_to_int(level: &tracing::Level) -> i32 {
  match *level {
    tracing::Level::TRACE => 4,
    tracing::Level::DEBUG => 3,
    tracing::Level::INFO => 2,
    tracing::Level::WARN => 1,
    tracing::Level::ERROR => 0,
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

  // Response fields/properties
  status: u64,
  latency_ms: f64,
  length: i64,

  // All other fields.
  fields: BTreeMap<String, serde_json::Value>,
}

struct LogJsonVisitor<'a>(&'a mut LogFieldStorage);

impl tracing::field::Visit for LogJsonVisitor<'_> {
  fn record_f64(&mut self, field: &Field, double: f64) {
    let name = field.name();
    match name {
      "latency_ms" => self.0.latency_ms = double,
      _ => {
        self.0.fields.insert(name.to_string(), json!(double));
      }
    };
  }

  fn record_i64(&mut self, field: &Field, int: i64) {
    let name = field.name();
    match name {
      "length" => self.0.length = int,
      _ => {
        self.0.fields.insert(name.to_string(), json!(int));
      }
    };
  }

  fn record_u64(&mut self, field: &Field, uint: u64) {
    let name = field.name();
    match name {
      "status" => self.0.status = uint,
      _ => {
        self.0.fields.insert(name.to_string(), json!(uint));
      }
    };
  }

  fn record_bool(&mut self, field: &Field, b: bool) {
    self.0.fields.insert(field.name().to_string(), json!(b));
  }

  fn record_str(&mut self, field: &Field, s: &str) {
    let name: &str = field.name();
    match name {
      "client_ip" => self.0.client_ip = s.to_string(),
      "host" => self.0.host = s.to_string(),
      "referer" => self.0.referer = s.to_string(),
      "user_agent" => self.0.user_agent = s.to_string(),
      name => {
        self.0.fields.insert(name.to_string(), json!(s));
      }
    };
  }

  fn record_debug(&mut self, field: &Field, dbg: &dyn std::fmt::Debug) {
    let name = field.name();
    let v = format!("{:?}", dbg);
    match name {
      "method" => self.0.method = v,
      "uri" => self.0.uri = v,
      "version" => self.0.version = v,
      name => {
        self.0.fields.insert(name.to_string(), json!(v));
      }
    };
  }

  fn record_error(&mut self, field: &Field, err: &(dyn std::error::Error + 'static)) {
    self
      .0
      .fields
      .insert(field.name().to_string(), json!(err.to_string()));
  }
}

// #[derive(Debug)]
// struct CustomFieldStorage(BTreeMap<String, serde_json::Value>);
//
// struct JsonVisitor<'a>(&'a mut BTreeMap<String, serde_json::Value>);
//
// impl<'a> tracing::field::Visit for JsonVisitor<'a> {
//   fn record_f64(&mut self, field: &Field, double: f64) {
//     self.0.insert(field.name().to_string(), json!(double));
//   }
//
//   fn record_i64(&mut self, field: &Field, int: i64) {
//     self.0.insert(field.name().to_string(), json!(int));
//   }
//
//   fn record_u64(&mut self, field: &Field, uint: u64) {
//     self.0.insert(field.name().to_string(), json!(uint));
//   }
//
//   fn record_bool(&mut self, field: &Field, b: bool) {
//     self.0.insert(field.name().to_string(), json!(b));
//   }
//
//   fn record_str(&mut self, field: &Field, s: &str) {
//     self.0.insert(field.name().to_string(), json!(s));
//   }
//
//   fn record_error(&mut self, field: &Field, err: &(dyn std::error::Error + 'static)) {
//     self
//       .0
//       .insert(field.name().to_string(), json!(err.to_string()));
//   }
//
//   fn record_debug(&mut self, field: &Field, dbg: &dyn std::fmt::Debug) {
//     self
//       .0
//       .insert(field.name().to_string(), json!(format!("{:?}", dbg)));
//   }
// }

fn get_header<'a>(headers: &'a HeaderMap, header_name: &'static str) -> Option<&'a str> {
  headers
    .get(header_name)
    .and_then(|header_value| header_value.to_str().ok())
}
