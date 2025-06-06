-- Migrate from UUIDv7 ids to truly random UUIDv4 ids + INTEGER primary key.
PRAGMA foreign_keys=off;

CREATE TABLE IF NOT EXISTS _new_user (
  pk                               INTEGER PRIMARY KEY,
  -- We only check `is_uuid` rather than `is_uuid_v4` to preserve user
  -- previously created as uuiv7.
  id                               BLOB NOT NULL CHECK(is_uuid(id)) DEFAULT (uuid_v4()),
  email                            TEXT NOT NULL CHECK(is_email(email)),
  password_hash                    TEXT DEFAULT '' NOT NULL,
  verified                         INTEGER DEFAULT FALSE NOT NULL,
  admin                            INTEGER DEFAULT FALSE NOT NULL,

  created                          INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  updated                          INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,

  -- Ephemeral data for auth flows.
  --
  -- Email change/verification flow.
  email_verification_code          TEXT,
  email_verification_code_sent_at  INTEGER,
  -- Change email flow.
  pending_email                    TEXT CHECK(is_email(pending_email)),
  -- Reset forgotten password flow.
  password_reset_code              TEXT,
  password_reset_code_sent_at      INTEGER,
  -- Authorization Code Flow (optionally with PKCE proof key).
  authorization_code               TEXT,
  authorization_code_sent_at       INTEGER,
  pkce_code_challenge              TEXT,

  -- OAuth metadata
  --
  -- provider_id maps to proto.config.OAuthProviderId enum.
  provider_id                      INTEGER DEFAULT 0 NOT NULL,
  -- The external provider's id for the user.
  provider_user_id                 TEXT,
  -- Link to an external avatar image for oauth providers only.
  provider_avatar_url              TEXT
) STRICT;

INSERT INTO _new_user(
    id, 
    email,
    password_hash,
    verified,
    admin,
    created,
    updated,
    email_verification_code,
    email_verification_code_sent_at,
    pending_email,
    password_reset_code,
    password_reset_code_sent_at,
    authorization_code,
    authorization_code_sent_at,
    pkce_code_challenge,
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
    email_verification_code,
    email_verification_code_sent_at,
    pending_email,
    password_reset_code,
    password_reset_code_sent_at,
    authorization_code,
    authorization_code_sent_at,
    pkce_code_challenge,
    provider_id,
    provider_user_id,
    provider_avatar_url
  FROM _user;

-- Create unique id index early to allow avatars to reference it.
CREATE UNIQUE INDEX __user__id_index ON _new_user (id);


-- Session table
CREATE TABLE IF NOT EXISTS _new_session (
  id                           INTEGER PRIMARY KEY,
  user                         INTEGER NOT NULL REFERENCES _new_user(pk) ON DELETE CASCADE,
  refresh_token                TEXT NOT NULL,
  updated                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

INSERT INTO _new_session(user, refresh_token, updated)
  SELECT user.pk, s.refresh_token, s.updated
  FROM 
    _session AS s LEFT JOIN _new_user AS user ON user.id = s.user;

DROP INDEX __session__refresh_token_index;
DROP INDEX __session__user_index;
DROP TRIGGER __session__updated_trigger;

DROP TABLE _session;

ALTER TABLE _new_session RENAME TO _session;

-- NOTE: The expiry is computed based on `updated` + TTL, thus touching the row
-- will extend the opaque refresh token's expiry.
CREATE TRIGGER __session__updated_trigger AFTER UPDATE ON _session FOR EACH ROW
  BEGIN
    UPDATE _session SET updated = UNIXEPOCH() WHERE user = OLD.user;
  END;

-- Main unique index to lookup refresh tokens efficiently.
CREATE UNIQUE INDEX __session__refresh_token_index ON _session (refresh_token);
-- An index on the user for efficient deletions of all sessions given a user.
CREATE INDEX __session__user_index ON _session (user);


-- Avatars
CREATE TABLE IF NOT EXISTS _new_user_avatar (
  id                           INTEGER PRIMARY KEY,
  user                         BLOB NOT NULL REFERENCES _new_user(id),
  file                         TEXT CHECK(jsonschema('std.FileUpload', file, 'image/png, image/jpeg')) NOT NULL,
  updated                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

INSERT INTO _new_user_avatar(user, file, updated)
  SELECT user, file, updated
  FROM _user_avatar;

DROP TRIGGER __user_avatar__updated_trigger;

DROP TABLE _user_avatar;

ALTER TABLE _new_user_avatar RENAME TO _user_avatar;

CREATE TRIGGER __user_avatar__updated_trigger AFTER UPDATE ON _user_avatar FOR EACH ROW
  BEGIN
    UPDATE _user_avatar SET updated = UNIXEPOCH() WHERE user = OLD.user;
  END;


-- Finally remove the old user table ane rename.
DROP INDEX __user__email_index;
DROP INDEX __user__email_verification_code_index;
DROP INDEX __user__password_reset_code_index;
DROP INDEX __user__authorization_code_index;
DROP INDEX __user__provider_ids_index;

DROP TRIGGER __user__updated_trigger;

DROP TABLE _user;

ALTER TABLE _new_user RENAME TO _user;

-- CREATE UNIQUE INDEX __user__id_index ON _user (id);
CREATE UNIQUE INDEX __user__email_index ON _user (email);
CREATE UNIQUE INDEX __user__email_verification_code_index ON _user (email_verification_code);
CREATE UNIQUE INDEX __user__password_reset_code_index ON _user (password_reset_code);
CREATE UNIQUE INDEX __user__authorization_code_index ON _user (authorization_code);
CREATE UNIQUE INDEX __user__provider_ids_index ON _user (provider_id, provider_user_id);

CREATE TRIGGER __user__updated_trigger AFTER UPDATE ON _user FOR EACH ROW
  BEGIN
    UPDATE _user SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;

PRAGMA foreign_keys=on;
