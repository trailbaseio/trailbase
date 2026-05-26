-- Pending file deletions
--
-- Triggers are being used to populate this deletion log. Keeping a log of
-- pending deletions also lets us retry deletions in case of transient errors.
CREATE TABLE _file_deletions (
  id                           BIGSERIAL PRIMARY KEY NOT NULL,
  deleted                      INT8 NOT NULL DEFAULT (UNIXEPOCH()),

  -- Cleanup metadata
  attempts                     INT8 NOT NULL DEFAULT 0,
  errors                       TEXT,

  -- Which record contained the file.
  table_name                   TEXT NOT NULL,
  record_rowid                 TID NOT NULL,
  column_name                  TEXT NOT NULL,

  -- File metadata, including id (path).
  json                         TEXT NOT NULL
);
