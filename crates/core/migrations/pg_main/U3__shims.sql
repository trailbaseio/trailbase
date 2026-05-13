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

-- It's a lie:
CREATE FUNCTION uuid_v7() RETURNS UUID AS $$
  BEGIN
    RETURN gen_random_uuid();
  END;
$$ LANGUAGE plpgsql;
