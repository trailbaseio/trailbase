-- Schema representing:
--   /docs/src/contents/docs/documentation/models_and_relations.

CREATE TABLE profile (
  id            BLOB NOT NULL PRIMARY KEY CHECK (is_uuid_v7(id)) DEFAULT (uuid_v7()),
  user          BLOB NOT NULL REFERENCES _user ON DELETE CASCADE UNIQUE,

  name          TEXT NOT NULL
) STRICT;

INSERT INTO profile (user, name) SELECT id, 'FirstUser' FROM _user WHERE email = '0@localhost';
INSERT INTO profile (user, name) SELECT id, 'SecondUser' FROM _user WHERE email = '1@localhost';

CREATE TABLE post (
  id            BLOB NOT NULL PRIMARY KEY CHECK (is_uuid_v7(id)) DEFAULT (uuid_v7()),
  author        BLOB NOT NULL REFERENCES _user,

  title         TEXT NOT NULL,
  body          TEXT NOT NULL
) STRICT;

INSERT INTO post (author, title, body) SELECT id, 'first post', 'body' FROM _user WHERE email = '0@localhost';
INSERT INTO post (author, title, body) SELECT id, 'second post', 'body' FROM _user WHERE email = '0@localhost';

CREATE TABLE tag (
  id            BLOB NOT NULL PRIMARY KEY CHECK (is_uuid_v7(id)) DEFAULT (uuid_v7()),
  label         TEXT NOT NULL
) STRICT;

INSERT INTO tag (label) VALUES ('important');
INSERT INTO tag (label) VALUES ('novel');

CREATE TABLE post_tag(
  post          BLOB NOT NULL REFERENCES post,
  tag           BLOB NOT NULL REFERENCES tag
) STRICT;

-- Add edge to label post with existing tags.
INSERT INTO post_tag (post, tag) SELECT post.id, tag.id FROM post, tag WHERE post.title = 'first post';

-- Create a view combining users and user profiles.
CREATE VIEW user_profile AS SELECT * FROM _user AS U LEFT JOIN profile AS P ON U.id = P.user;

-- Create a view combining posts with tags.
CREATE VIEW post_tag_view AS SELECT * FROM post AS P LEFT JOIN post_tag AS PT ON P.id = PT.post LEFT JOIN tag AS T ON T.ID = PT.tag;
