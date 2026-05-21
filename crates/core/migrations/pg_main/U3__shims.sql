-- Fake SQLite functionality.
CREATE VIEW pragma_database_list AS
  SELECT 0 AS seq, 'public' AS name;

CREATE VIEW sqlite_schema AS
  SELECT 'table' AS "type", 'test_table' AS "name", 'test_table' AS tbl_name, 2 AS rootpage, 'CREATE TABLE test_table (id INTEGER PRIMARY KEY) STRICT' AS sql;

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

CREATE FUNCTION jsonschema(n TEXT, column_name TEXT) RETURNS BOOL AS $$
  BEGIN
    -- It's another lie:
    RETURN TRUE;
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
