CREATE TABLE articles (
    id           BLOB PRIMARY KEY NOT NULL CHECK(is_uuid_v7(id)) DEFAULT (uuid_v7()),
    author       BLOB NOT NULL REFERENCES _user(id) ON DELETE CASCADE,

    title        TEXT NOT NULL,
    intro        TEXT NOT NULL,
    tag          TEXT NOT NULL,
    body         TEXT NOT NULL,

    image        TEXT CHECK(jsonschema('std.FileUpload', image, 'image/png, image/jpeg')),

    created      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL
) STRICT;

-- Join articles with user profiles to get the username.
CREATE VIEW articles_view AS SELECT a.*, p.username FROM articles AS a LEFT JOIN profiles AS p ON p.user = a.author;
