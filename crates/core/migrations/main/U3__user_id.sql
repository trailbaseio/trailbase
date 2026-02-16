-- Migrate from UUIDv7 ids to truly random UUIDv4 ids + INTEGER primary key.
PRAGMA foreign_keys=off;

CREATE TABLE IF NOT EXISTS _new_user (
  -- We only check `is_uuid` rather than `is_uuid_v4` to preserve user
  -- previously created as uuiv7.
  id                               BLOB PRIMARY KEY NOT NULL CHECK(is_uuid(id)) DEFAULT (uuid_v4()),
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
  -- OTP flow.
  otp_code                         TEXT,
  otp_sent_at                      INTEGER,

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
    otp_code,
    otp_sent_at,
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
    otp_code,
    otp_sent_at,
    provider_id,
    provider_user_id,
    provider_avatar_url
  FROM _user;

DROP TABLE _user;

ALTER TABLE _new_user RENAME TO _user;

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
