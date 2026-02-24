-- Add OTP/TOTP columns.

-- Defer any foreign key integrity checks within this transaction until it's being commited.
PRAGMA defer_foreign_keys = ON;

CREATE TABLE IF NOT EXISTS _new_with_otp (
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
  -- TOTP secret for authenticator.
  totp_secret                      TEXT,

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

-- We need to turn ON "legacy" behavior to not upset any indexes or views
-- pointing at _user.
PRAGMA legacy_alter_table=ON;

DROP TABLE _user;
ALTER TABLE _new_with_otp RENAME TO _user;

-- Turn OFF legacy behavior.
PRAGMA legacy_alter_table=OFF;

CREATE TRIGGER __user__updated_trigger AFTER UPDATE ON _user FOR EACH ROW
  BEGIN
    UPDATE _user SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;
