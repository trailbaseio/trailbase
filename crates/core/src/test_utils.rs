use trailbase_sqlite::{Connection, ConnectionType};

pub use crate::util::row_id_column;

pub fn strict(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "",
    ConnectionType::Sqlite => "STRICT",
  };
}

pub fn uuid_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "UUID",
    ConnectionType::Sqlite => "BLOB",
  };
}

pub fn blob_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "BYTEA",
    ConnectionType::Sqlite => "BLOB",
  };
}

pub fn json_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "JSONB",
    ConnectionType::Sqlite => "TEXT",
  };
}

pub fn serial_column(conn: &Connection) -> &'static str {
  return match conn.connection_type() {
    ConnectionType::Pg => "BIGSERIAL",
    ConnectionType::Sqlite => "INTEGER",
  };
}
