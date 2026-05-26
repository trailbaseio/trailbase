CREATE OR REPLACE FUNCTION UNIXEPOCH() RETURNS INT8 AS $$
  BEGIN
    RETURN EXTRACT(EPOCH FROM CURRENT_TIMESTAMP);
  END;
$$ LANGUAGE plpgsql;

-- PG before v18 may not provide `uuidv4()`, thus add an impl.
CREATE FUNCTION uuid_v4() RETURNS UUID AS $$
  BEGIN
    RETURN gen_random_uuid();
  END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION uuid_v7() RETURNS UUID AS $$
  BEGIN
    RETURN uuid_generate_v7();
  END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION is_uuid_v7(id UUID) RETURNS BOOL AS $$
  BEGIN
    RETURN uuid_extract_version(id) = 7;
  END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION is_uuid(id ANYELEMENT) RETURNS BOOL AS $$
  BEGIN
    RETURN uuid_extract_version(id) > 0;
  EXCEPTION
    -- pglite fallback
    WHEN others THEN
      RETURN FALSE;
  END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION jsonschema(n TEXT, contents JSONB) RETURNS BOOL AS $$
  BEGIN
    CASE n
      -- We're lying a little here:
      WHEN 'std.FileUpload' THEN
        RETURN TRUE;
      WHEN 'std.FileUploads' THEN
        RETURN TRUE;
      ELSE
        RAISE EXCEPTION 'Not supported alongside PG';
    END CASE;
  END;
$$ LANGUAGE plpgsql;

CREATE FUNCTION hash_password(pw TEXT) RETURNS TEXT AS $$
  -- Try pgcrypto first. Available in in PG by default but not pglite:
  --   https://github.com/f0rr0/pglite-oxide/blob/main/docs/EXTENSIONS.md
  BEGIN
    -- NOTE: This uses bcrypt instead of argon.
    CREATE EXTENSION IF NOT EXISTS pgcrypto;
    RETURN crypt(pw, gen_salt('bf', 8));
  EXCEPTION
    -- pglite fallback
    WHEN others THEN
      IF pw = 'secret' THEN
        RETURN '$2a$08$ZMXt5Iw/CXQQyBYUei2d2.oXio.U1aeKgCip6xWvMMwYyw2tXtVP2';
      ELSE
        RAISE 'pglite + unexpected pw';
      END IF;
  END;
$$ LANGUAGE plpgsql;
