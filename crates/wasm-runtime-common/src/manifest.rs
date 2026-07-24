use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum HttpMethodType {
  Get,
  Post,
  Head,
  Options,
  Patch,
  Delete,
  Put,
  Trace,
  Connect,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum Subsystem {
  Metadata,
  Http,
  Jobs,
  SqliteFunctions,
  #[serde(other)]
  Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS)]
#[serde(rename_all = "snake_case")]
pub enum GuestRuntime {
  Rust,
  EcmaScript,
  #[serde(other)]
  Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct HttpRoute {
  pub method: HttpMethodType,
  pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct Job {
  pub name: String,
  pub spec: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, TS)]
pub enum SqliteFunctionFlag {
  /// Specifies UTF-8 as the text encoding this SQL function prefers for its parameters.
  Utf8,
  /// Specifies UTF-16 using little-endian byte order as the text encoding this SQL function prefers
  /// for its parameters.
  Utf16le,
  /// Specifies UTF-16 using big-endian byte order as the text encoding this SQL function prefers
  /// for its parameters.
  Utf16be,
  /// Specifies UTF-16 using native byte order as the text encoding this SQL function prefers for
  /// its parameters.
  Utf16,
  /// Means that the function always gives the same output when the input parameters are the same.
  Deterministic,
  /// Means that the function may only be invoked from top-level SQL.
  DirectOnly,
  /// Indicates to SQLite that a function may call `sqlite3_value_subtype()` to inspect the subtypes
  /// of its arguments.
  Subtype,
  /// Means that the function is unlikely to cause problems even if misused.
  Innocuous,
  /// Indicates to SQLite that a function might call `sqlite3_result_subtype()` to cause a subtype
  /// to be associated with its result.
  ResultSubtype,
  /// Indicates that the function is an aggregate that internally orders the values provided to the
  /// first argument.
  Selforder1,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct SqliteScalarFunction {
  pub name: String,
  pub num_args: u32,
  pub flags: Vec<SqliteFunctionFlag>,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub enum SqliteFunction {
  Scalar(SqliteScalarFunction),
}

/// Metadata a component can self-report for display in the admin WASM modules page.
#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
pub struct Metadata {
  /// Name to show in the admin UI component browser.
  pub display_name: Option<String>,
  /// Optional description to show in the admin UI.
  pub description: Option<String>,
  /// Icon to show in the admin UI component browser, e.g. "<svg ..." or
  /// "data:<mime-type>;base64,<base64-encoded-data>".
  pub icon: Option<String>,
  /// Which guest runtime is used by the component.
  pub guest_runtime: Option<GuestRuntime>,
  /// An HTTP endpoint for an admin UI to present in the admin dashboard's, e.g.: "/comp0/admin".
  pub admin_ui_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct InitArguments {
  // Host version.
  pub version: Option<String>,

  // List of subsystems to initialize.
  pub subsystems: Option<Vec<Subsystem>>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct InitManifest {
  /// Metadata for the WASM component, useful e.g. for the admin page.
  pub metadata: Option<Metadata>,

  /// Registered HTTP handlers tuple of Method + Path. May contain wild cards.
  pub http_handlers: Option<Vec<HttpRoute>>,

  /// Registered JobHandlers.
  pub job_handlers: Option<Vec<Job>>,

  /// Registered Sqlite functions.
  pub sqlite_functions: Option<Vec<SqliteFunction>>,
}
