-- Table with custom profile information.
--
-- One could add more user information here, customize validation, etc.
CREATE TABLE profiles (
    user         BLOB PRIMARY KEY NOT NULL REFERENCES _user(id) ON DELETE CASCADE,

    -- Make sure that usernames are at least 3 alphanumeric characters.
    username     TEXT NOT NULL CHECK(username REGEXP '^[\w]{3,}$'),

    created      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
    updated      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

-- Ensure usernames are unique.
CREATE UNIQUE INDEX _profiles__username_index ON profiles (username);

-- Use trigger to manage the `updated` timestamp.
CREATE TRIGGER _profiles__updated_trigger AFTER UPDATE ON profiles FOR EACH ROW
  BEGIN
    UPDATE profiles SET updated = UNIXEPOCH() WHERE user = OLD.user;
  END;

-- Compile username, avatar_url, and is_editor into a single convenient
-- read-only API.
CREATE VIEW profiles_view AS
  SELECT
    p.*,
    -- TrailBase requires top-level cast to determine result type and generate JSON schemas.
    CAST(CASE
      WHEN avatar.file IS NOT NULL THEN CONCAT('/api/auth/avatar/', uuid_text(p.user))
      ELSE NULL
    END AS TEXT) AS avatar_url,
    -- TrailBase requires top-level cast to determine result type and generate JSON schemas.
    CAST(IIF(editors.user IS NULL, FALSE, TRUE) AS BOOLEAN) AS is_editor
  FROM profiles AS p
    LEFT JOIN _user_avatar AS avatar ON p.user = avatar.user
    LEFT JOIN editors ON p.user = editors.user;
