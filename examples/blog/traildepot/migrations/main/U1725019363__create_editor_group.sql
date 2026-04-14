-- Create a group that is used to gate write access to articles. Members of
-- this group can author articles.
CREATE TABLE editors (
  user BLOB NOT NULL,

  FOREIGN KEY(user) REFERENCES _user(id) ON DELETE CASCADE
) STRICT;
