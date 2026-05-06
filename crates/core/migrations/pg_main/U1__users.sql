CREATE TABLE IF NOT EXISTS _user (
  -- We only check `is_uuid` rather than `is_uuid_v4` to preserve user
  -- previously created as uuiv7.
  id                               BLOB PRIMARY KEY NOT NULL DEFAULT (uuidv4()),
  email                            TEXT NOT NULL,
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
);

CREATE UNIQUE INDEX __user__email_index ON _user (email);
CREATE UNIQUE INDEX __user__provider_ids_index ON _user (provider_id, provider_user_id);

CREATE TRIGGER __user__updated_trigger AFTER UPDATE ON _user FOR EACH ROW
  BEGIN
    UPDATE _user SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;


--
-- User avatar table
--
CREATE TABLE _user_avatar (
  user                         BLOB PRIMARY KEY NOT NULL REFERENCES _user(id) ON DELETE CASCADE,
  file                         TEXT NOT NULL,
  updated                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

CREATE TRIGGER __user_avatar__updated_trigger AFTER UPDATE ON _user_avatar FOR EACH ROW
  BEGIN
    UPDATE _user_avatar SET updated = UNIXEPOCH() WHERE user = OLD.user;
  END;
