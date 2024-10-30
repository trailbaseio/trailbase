# About the Dataset.

This dataset was originally compiled from [IMDB
data](https://developer.imdb.com/non-commercial-datasets/) and is subject to
their terms of use and any applicable legal restrictions..

The compilation is provided by
[kaggle](https://www.kaggle.com/datasets/inductiveanks/top-1000-imdb-movies-dataset/data)
and can be downloaded via:

```bash
curl -o archive.zip https://www.kaggle.com/api/v1/datasets/download/inductiveanks/top-1000-imdb-movies-dataset
```

## Schema

```sql
CREATE TABLE movies (
  rank         INTEGER PRIMARY KEY,
  name         TEXT NOT NULL,

  -- Year cannot be INTEGER, since some are like "I 2016".
  year         ANY NOT NULL,
  watch_time   INTEGER NOT NULL, -- in minutes
  rating       REAL NOT NULL,

  -- Ideally nullable integer, however sqlite assumes empty to be text.
  metascore    ANY,

  -- Ideally nullable real, however sqlite assumes empty to be text.
  gross        ANY,
  votes        TEXT NOT NULL,
  description  TEXT NOT NULL
) STRICT;
```
