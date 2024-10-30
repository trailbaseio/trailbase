CREATE TABLE IF NOT EXISTS _logs (
  -- NOTE: We're skipping the CHECK(is_uuid_v7) here as mostly inconsequential
  -- micro-optimization but we also don't wanna expose the logs via RecordAPIs,
  -- so there's not strict need.
  id                           BLOB PRIMARY KEY DEFAULT (uuid_v7()) NOT NULL,
  -- Timestamp in seconds with fractional millisecond resolution.
  created                      REAL DEFAULT (UNIXEPOCH('subsec')) NOT NULL,
  -- Entry type. We could probably also split by table :shrug:
  type                         INTEGER DEFAULT 0 NOT NULL,

  level                        INTEGER DEFAULT 0 NOT NULL,
  status                       INTEGER DEFAULT 0 NOT NULL,
  method                       TEXT DEFAULT '' NOT NULL,
  url                          TEXT DEFAULT '' NOT NULL,

  latency                      REAL DEFAULT 0 NOT NULL,

  client_ip                    TEXT DEFAULT '' NOT NULL,
  referer                      TEXT DEFAULT '' NOT NULL,
  user_agent                   TEXT DEFAULT '' NOT NULL,

  data                         BLOB
) strict;

CREATE INDEX IF NOT EXISTS __logs__level_index ON _logs (level);
CREATE INDEX IF NOT EXISTS __logs__created_index ON _logs (created);
CREATE INDEX IF NOT EXISTS __logs__status_index ON _logs (status);
CREATE INDEX IF NOT EXISTS __logs__method_index ON _logs (method);
