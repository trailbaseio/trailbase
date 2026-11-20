CREATE TABLE test_table (
  id     BLOB PRIMARY KEY CHECK (is_uuid_v4(id)) DEFAULT (uuid_v4()) NOT NULL,
  data   TEXT NOT NULL DEFAULT '<default>'
) STRICT;
