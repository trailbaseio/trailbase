-- Pending file deletions
--
-- Triggers are being used to populate this deletion log. Keeping a log of
-- pending deletions also lets us retry deletions in case of transient errors.
CREATE TABLE _file_deletions (
  id                           INTEGER PRIMARY KEY NOT NULL,
  deleted                      INTEGER NOT NULL DEFAULT (UNIXEPOCH()),

  -- Cleanup metadata
  attempts                     INTEGER NOT NULL DEFAULT 0,
  errors                       TEXT,

  -- Which record contained the file.
  table_name                   TEXT NOT NULL,
  record_rowid                 INTEGER NOT NULL,
  column_name                  TEXT NOT NULL,

  -- File metadata, including id (path).
  json                         TEXT NOT NULL
) STRICT;
