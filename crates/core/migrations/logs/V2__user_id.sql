PRAGMA foreign_keys=off;

-- Add user_id col remove level & type, as well as switch to integer id.
CREATE TABLE IF NOT EXISTS _new_logs (
  id                           INTEGER PRIMARY KEY,

  -- Timestamp in seconds with fractional millisecond resolution.
  created                      REAL DEFAULT (UNIXEPOCH('subsec')) NOT NULL,

  status                       INTEGER DEFAULT 0 NOT NULL,
  method                       TEXT DEFAULT '' NOT NULL,
  url                          TEXT DEFAULT '' NOT NULL,

  latency                      REAL DEFAULT 0 NOT NULL,

  client_ip                    TEXT DEFAULT '' NOT NULL,
  referer                      TEXT DEFAULT '' NOT NULL,
  user_agent                   TEXT DEFAULT '' NOT NULL,

  -- Ideally we would use "REFERENCES _user(id) ON DELETE SET NULL",
  -- however logs and users are in separate databases.
  user_id                      BLOB,

  data                         TEXT
) STRICT;

INSERT INTO _new_logs(created, status, method, url, latency, client_ip, referer, user_agent, data)
  SELECT created, status, method, url, latency, client_ip, referer, user_agent, data FROM _logs;

DROP INDEX  __logs__level_index;
DROP INDEX  __logs__created_index;
DROP INDEX  __logs__status_index;
DROP INDEX  __logs__method_index;

DROP TABLE _logs;

ALTER TABLE _new_logs RENAME TO _logs;

CREATE INDEX IF NOT EXISTS __logs__created_index ON _logs (created);
CREATE INDEX IF NOT EXISTS __logs__status_index ON _logs (status);
CREATE INDEX IF NOT EXISTS __logs__method_index ON _logs (method);

PRAGMA foreign_keys=on;
