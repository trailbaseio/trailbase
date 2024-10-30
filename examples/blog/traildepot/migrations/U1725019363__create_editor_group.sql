-- Create a group that is used to gate write access to articles. Members of
-- this group can author articles.
CREATE TABLE editors (
  user BLOB NOT NULL,

  FOREIGN KEY(user) REFERENCES _user(id) ON DELETE CASCADE
) STRICT;

-- Create an "is_editor" query api.
CREATE VIRTUAL TABLE _is_editor USING define((SELECT EXISTS (SELECT * FROM editors WHERE user = $1) AS is_editor));
