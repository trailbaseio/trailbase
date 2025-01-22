-- A table schema to hold the IMDB test dataset from:
--   https://www.kaggle.com/datasets/inductiveanks/top-1000-imdb-movies-dataset/data
--
-- The only TrailBase API requirements are: "STRICT" typing and a INTEGER (or
-- UUIDv7) PRIMARY KEY column.
CREATE TABLE IF NOT EXISTS movies (
  rank         INTEGER PRIMARY KEY,
  name         TEXT NOT NULL,
  year         ANY NOT NULL,
  watch_time   INTEGER NOT NULL,
  rating       REAL NOT NULL,
  metascore    ANY,
  gross        ANY,
  votes        TEXT NOT NULL,
  description  TEXT NOT NULL
) STRICT;
