CREATE TABLE IF NOT EXISTS counter (
  id           INTEGER PRIMARY KEY,
  value        INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO counter (id, value) VALUES (1, 5);
