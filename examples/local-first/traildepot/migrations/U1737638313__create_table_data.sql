CREATE TABLE IF NOT EXISTS data (
  id           INTEGER PRIMARY KEY,
  updated      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  data         TEXT NOT NULL
) STRICT;

CREATE TRIGGER __data__updated_trigger AFTER UPDATE ON data FOR EACH ROW
  BEGIN
    UPDATE data SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;

INSERT INTO data (data) VALUES ('0'), ('1');
