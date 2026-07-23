CREATE TABLE _wasm_shared_preferences (
  -- The component's name.
  component   TEXT NOT NULL,
  -- actual value, e.g. JSON, proto, ...
  value       TEXT
) STRICT;

CREATE UNIQUE INDEX __wasm_shared_preferences__component ON _wasm_shared_preferences (component);
