-- Add `totp_secret` column and remove columns containing ephemeral,
-- session-like state in favor of JWT and a separate "session" DB.

CREATE TABLE IF NOT EXISTS _new_with_otp (
  -- We only check `is_uuid` rather than `is_uuid_v4` to preserve user
  -- previously created as uuiv7.
  id                               BLOB PRIMARY KEY NOT NULL CHECK(is_uuid(id)) DEFAULT (uuid_v4()),
  email                            TEXT NOT NULL CHECK(is_email(email)),
  password_hash                    TEXT DEFAULT '' NOT NULL,
  verified                         INTEGER DEFAULT FALSE NOT NULL,
  admin                            INTEGER DEFAULT FALSE NOT NULL,

  -- TOTP secret for authenticator.
  totp_secret                      TEXT,

  created                          INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  updated                          INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,

  -- OAuth metadata
  --
  -- provider_id maps to proto.config.OAuthProviderId enum.
  provider_id                      INTEGER DEFAULT 0 NOT NULL,
  -- The external provider's id for the user.
  provider_user_id                 TEXT,
  -- Link to an external avatar image for oauth providers only.
  provider_avatar_url              TEXT
) STRICT;

INSERT INTO _new_with_otp(
    id,
    email,
    password_hash,
    verified,
    admin,
    created,
    updated,
    provider_id,
    provider_user_id,
    provider_avatar_url
  )
  SELECT
    id,
    email,
    password_hash,
    verified,
    admin,
    created,
    updated,
    provider_id,
    provider_user_id,
    provider_avatar_url
  FROM _user;

-- Turn ON legacy behavior to avoid issues with VIEWs referencing the _user
-- table. FOREIGN_KEY constraints are handled by refinery.
PRAGMA legacy_alter_table=ON;

DROP TABLE _user;
ALTER TABLE _new_with_otp RENAME TO _user;

-- Turn OFF legacy behavior.
PRAGMA legacy_alter_table=OFF;

-- Re-create INDEXes and TRIGGERs removed by `DROP TABLE`.
CREATE UNIQUE INDEX __user__email_index ON _user (email);
CREATE UNIQUE INDEX __user__provider_ids_index ON _user (provider_id, provider_user_id);

CREATE TRIGGER __user__updated_trigger AFTER UPDATE ON _user FOR EACH ROW
  BEGIN
    UPDATE _user SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;
