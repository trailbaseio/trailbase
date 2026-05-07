-- PG before v18 may not provide `uuidv4()`, thus add an impl.
CREATE FUNCTION uuidv4() RETURNS UUID AS $$
  BEGIN
    RETURN gen_random_uuid();
  END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION UNIXEPOCH() RETURNS INT8 AS $$
  BEGIN
    RETURN EXTRACT(EPOCH FROM CURRENT_TIMESTAMP);
  END;
$$ LANGUAGE plpgsql;

CREATE TABLE IF NOT EXISTS _user (
  id                               UUID PRIMARY KEY NOT NULL DEFAULT (uuidv4()),
  email                            TEXT NOT NULL,
  password_hash                    TEXT DEFAULT '' NOT NULL,
  verified                         BOOLEAN DEFAULT FALSE NOT NULL,
  admin                            BOOLEAN DEFAULT FALSE NOT NULL,

  -- TOTP secret for authenticator.
  totp_secret                      TEXT,

  created                          INT8 DEFAULT (UNIXEPOCH()) NOT NULL,
  updated                          INT8 DEFAULT (UNIXEPOCH()) NOT NULL,

  -- OAuth metadata
  --
  -- provider_id maps to proto.config.OAuthProviderId enum.
  provider_id                      INT8 DEFAULT 0 NOT NULL,
  -- The external provider's id for the user.
  provider_user_id                 TEXT,
  -- Link to an external avatar image for oauth providers only.
  provider_avatar_url              TEXT
);

CREATE UNIQUE INDEX __user__email_index ON _user (email);
CREATE UNIQUE INDEX __user__provider_ids_index ON _user (provider_id, provider_user_id);

CREATE OR REPLACE FUNCTION __user__updated_function() RETURNS TRIGGER AS $$
  BEGIN
    NEW.updated = UNIXEPOCH();
    RETURN NEW;
  END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER __user__updated_trigger BEFORE UPDATE ON _user
  FOR EACH ROW
  -- Prevent trigger triggering itself.
  WHEN (pg_trigger_depth() < 1)
  EXECUTE FUNCTION __user__updated_function();


--
-- User avatar table
--
CREATE TABLE IF NOT EXISTS _user_avatar (
  "user"                       UUID PRIMARY KEY NOT NULL REFERENCES _user(id) ON DELETE CASCADE,
  file                         TEXT NOT NULL,
  updated                      INT8 DEFAULT (UNIXEPOCH()) NOT NULL
);

CREATE OR REPLACE FUNCTION __user_avatar__updated_function() RETURNS TRIGGER AS $$
  BEGIN
    NEW.updated = UNIXEPOCH();
    RETURN NEW;
  END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER __user_avatar__updated_trigger BEFORE UPDATE ON _user_avatar
  FOR EACH ROW
  -- Prevent trigger triggering itself.
  WHEN (pg_trigger_depth() < 1)
  EXECUTE FUNCTION __user_avatar__updated_function();
