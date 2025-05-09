use chrono::Duration;

pub const SQLITE_SCHEMA_TABLE: &str = "main.sqlite_schema";
pub const USER_TABLE: &str = "_user";
pub(crate) const USER_TABLE_ID_COLUMN: &str = "id";

pub(crate) const SESSION_TABLE: &str = "_session";
pub(crate) const AVATAR_TABLE: &str = "_user_avatar";

pub(crate) const LOGS_TABLE_ID_COLUMN: &str = "id";
pub const LOGS_RETENTION_DEFAULT: Duration = Duration::days(7);

pub const COOKIE_AUTH_TOKEN: &str = "auth_token";
pub const COOKIE_REFRESH_TOKEN: &str = "refresh_token";
pub const COOKIE_OAUTH_STATE: &str = "oauth_state";

// NOTE: We're using the standard "Authorization" header for the JWT auth token. Custom header
// naming: https://datatracker.ietf.org/doc/html/draft-saintandre-xdash-00
pub const HEADER_REFRESH_TOKEN: &str = "Refresh-Token";
pub const HEADER_CSRF_TOKEN: &str = "CSRF-Token";

#[cfg(debug_assertions)]
pub const DEFAULT_AUTH_TOKEN_TTL: Duration = Duration::minutes(2);
#[cfg(not(debug_assertions))]
pub const DEFAULT_AUTH_TOKEN_TTL: Duration = Duration::minutes(60);

pub const DEFAULT_REFRESH_TOKEN_TTL: Duration = Duration::days(30);

pub(crate) const VERIFICATION_CODE_LENGTH: usize = 24;
pub(crate) const REFRESH_TOKEN_LENGTH: usize = 32;

// Public APIs
pub const RECORD_API_PATH: &str = "api/records/v1";
pub const QUERY_API_PATH: &str = "api/query/v1";
pub const AUTH_API_PATH: &str = "api/auth/v1";
pub const ADMIN_API_PATH: &str = "api/_admin";
