CREATE TABLE todos (
  "id"          INTEGER PRIMARY KEY NOT NULL,
  "text"        TEXT NOT NULL,
  "completed"   INTEGER NOT NULL DEFAULT 0,
  "created_at"  INTEGER NOT NULL DEFAULT(UNIXEPOCH()),
  "updated_at"  INTEGER NOT NULL DEFAULT(UNIXEPOCH())
) STRICT;

CREATE TRIGGER _todos__update_trigger AFTER UPDATE ON todos FOR EACH ROW
  BEGIN
    UPDATE todos SET updated_at = UNIXEPOCH() WHERE id = OLD.id;
  END;
