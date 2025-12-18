CREATE TABLE IF NOT EXISTS simple_schema_table (
  id             INTEGER PRIMARY KEY,
  data           TEXT NOT NULL CHECK(jsonschema('simple_schema', data)) DEFAULT '{ "name": "Alice" }'
) STRICT;
