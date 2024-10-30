-- Create a table that doesn't satisfy record API requirements and uses
-- "affinity names" rather than strict storage types.
CREATE TABLE non_strict_table (
  id            INTEGER PRIMARY KEY NOT NULL,

  tinyint_col   TINYINT,
  bigint_col    BIGINT,

  varchar_col   VARCHAR(64),
  double_col    DOUBLE,
  float_col     FLOAT,

  boolean_col   BOOLEAN,
  date_col      DATE,
  datetime_col  DATETIME
);

INSERT INTO non_strict_table
  (id, tinyint_col, bigint_col, varchar_col, double_col, float_col, boolean_col, date_col, datetime_col)
VALUES
  (0, 5, 64, 'varchar', 5.2, 2.4, FALSE, UNIXEPOCH(), UNIXEPOCH()),
  (1, 5, 64, 'varchar', 5.2, 2.4, FALSE, UNIXEPOCH(), UNIXEPOCH()),
  (2, 5, 64, 'varchar', 5.2, 2.4, FALSE, UNIXEPOCH(), UNIXEPOCH()),
  (NULL, 5, 64, 'varchar', 5.2, 2.4, FALSE, UNIXEPOCH(), UNIXEPOCH());

CREATE TABLE non_strict_autoincrement_table (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  int4_col      INT4
);

INSERT INTO non_strict_autoincrement_table (int4_col) VALUES (12);
