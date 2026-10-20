CREATE TABLE IF NOT EXISTS _tenant (
  -- Tenant UUIDv4.
  id                    BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v4(id)) DEFAULT (uuid_v4()),
  -- Template id of a template defined in config.textproto.
  template              TEXT NOT NULL,

  created               INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

-- CREATE TRIGGER __tenant__updated_trigger AFTER UPDATE ON _tenant FOR EACH ROW
--   BEGIN
--     UPDATE _tenant SET updated = UNIXEPOCH() WHERE id = OLD.id;
--   END;

CREATE TABLE IF NOT EXISTS _membership (
  tenant                BLOB NOT NULL REFERENCES _tenant ON DELETE CASCADE,
  user                  BLOB NOT NULL REFERENCES _user ON DELETE CASCADE,

  created               INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

CREATE UNIQUE INDEX __membership__tenant_index ON _membership (tenant);
CREATE UNIQUE INDEX __membership__user_index ON _membership (user);
