CREATE TABLE _wasm_shared_preferences (
  -- The component's name.
  component   TEXT NOT NULL,
  -- JSON map of [key: string].
  value       JSONB CHECK(jsonschema('std.KeyValue', value))
);

CREATE UNIQUE INDEX __wasm_shared_preferences__component ON _wasm_shared_preferences (component);
