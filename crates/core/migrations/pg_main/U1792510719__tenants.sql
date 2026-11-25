CREATE TABLE IF NOT EXISTS _tenant (
  -- Stable UUIDv4 tenant id.
  id                    UUID PRIMARY KEY NOT NULL DEFAULT (uuid_v4()),

  -- A human-readable name.
  name                  TEXT NOT NULL,

  -- Template id of a template defined in config.textproto.
  template              TEXT NOT NULL,

  created               INT8 DEFAULT (UNIXEPOCH()) NOT NULL
);

CREATE TABLE IF NOT EXISTS _tenant_member (
  -- Id of tenant.
  tenant                UUID NOT NULL REFERENCES _tenant ON DELETE CASCADE,
  -- Id of tenant member.
  "user"                UUID NOT NULL REFERENCES _user ON DELETE CASCADE,

  created               INT8 DEFAULT (UNIXEPOCH()) NOT NULL
);

CREATE UNIQUE INDEX __membership__tenant_index ON _tenant_member (tenant);
CREATE UNIQUE INDEX __membership__user_index ON _tenant_member ("user");
