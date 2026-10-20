CREATE TABLE IF NOT EXISTS _tenant (
  -- Stable UUIDv4 tenant id.
  id                    BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v4(id)) DEFAULT (uuid_v4()),

  -- A human-readable name.
  name                  TEXT NOT NULL,

  -- Template id of a template defined in config.textproto.
  template              TEXT NOT NULL,

  created               INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS _tenant_member (
  -- Id of tenant.
  tenant                BLOB NOT NULL REFERENCES _tenant ON DELETE CASCADE,
  -- Id of tenant member.
  user                  BLOB NOT NULL REFERENCES _user ON DELETE CASCADE,

  created               INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

CREATE UNIQUE INDEX __membership__tenant_index ON _tenant_member (tenant);
CREATE UNIQUE INDEX __membership__user_index ON _tenant_member (user);
