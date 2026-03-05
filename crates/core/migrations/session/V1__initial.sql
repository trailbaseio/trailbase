--
-- Session table.
--
CREATE TABLE _session (
  id                           INTEGER PRIMARY KEY NOT NULL,
  user                         BLOB NOT NULL,
  refresh_token                TEXT NOT NULL,
  created                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  expires                      INTEGER  NOT NULL
) STRICT;

-- Main unique index to lookup refresh tokens efficiently.
CREATE UNIQUE INDEX __session__refresh_token_index ON _session (refresh_token);

-- An index on the user for efficient deletions of all sessions given a user.
-- NOTE: The index is not UNIQUE, i.e. users may hold multiple sessions, e.g.
-- from multiple devices.
CREATE INDEX __session__user_index ON _session (user);

--
-- Authorization codes for PKCE login.
--
CREATE TABLE _authorization_code (
  id                           INTEGER PRIMARY KEY NOT NULL,
  user                         BLOB NOT NULL,
  authorization_code           TEXT,
  pkce_code_challenge          TEXT,
  created                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  expires                      INTEGER  NOT NULL
) STRICT;

-- Main auth-code lookup.
CREATE UNIQUE INDEX __authorization_code__code ON _authorization_code (authorization_code);
