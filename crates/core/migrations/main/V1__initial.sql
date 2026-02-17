--
-- User table.
--
CREATE TABLE _user (
  id                               BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT (uuid_v7()),
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

CREATE UNIQUE INDEX __user__email_index ON _user (email);
CREATE UNIQUE INDEX __user__email_verification_code_index ON _user (email_verification_code);
CREATE UNIQUE INDEX __user__password_reset_code_index ON _user (password_reset_code);
CREATE UNIQUE INDEX __user__authorization_code_index ON _user (authorization_code);
CREATE UNIQUE INDEX __user__provider_ids_index ON _user (provider_id, provider_user_id);

CREATE TRIGGER __user__updated_trigger AFTER UPDATE ON _user FOR EACH ROW
  BEGIN
    UPDATE _user SET updated = UNIXEPOCH() WHERE id = OLD.id;
  END;

--
-- Session table
--
CREATE TABLE _session (
  id                           INTEGER PRIMARY KEY NOT NULL,
  user                         BLOB NOT NULL REFERENCES _user(id) ON DELETE CASCADE,
  refresh_token                TEXT NOT NULL,
  updated                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

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

--
-- User avatar table
--
CREATE TABLE _user_avatar (
  user                         BLOB PRIMARY KEY NOT NULL REFERENCES _user(id) ON DELETE CASCADE,
  file                         TEXT CHECK(jsonschema('std.FileUpload', file, 'image/png, image/jpeg')) NOT NULL,
  updated                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

CREATE TRIGGER __user_avatar__updated_trigger AFTER UPDATE ON _user_avatar FOR EACH ROW
  BEGIN
    UPDATE _user_avatar SET updated = UNIXEPOCH() WHERE user = OLD.user;
  END;
