CREATE TABLE comment (
  id            INTEGER PRIMARY KEY,

  -- Post the comment belongs to.
  post          BLOB NOT NULL REFERENCES post ON DELETE CASCADE,
  -- Author of the comment (not the post).
  author        BLOB NOT NULL REFERENCES profile ON DELETE CASCADE,

  -- Comment contents.
  body          TEXT NOT NULL
) STRICT;

INSERT INTO comment (id, post, author, body) SELECT 1, post.id, profile.id, 'first comment'
  FROM
    (SELECT id FROM post LIMIT 1) AS post,
    (SELECT id FROM profile WHERE name = 'SecondUser') AS profile;

INSERT INTO comment (id, post, author, body) SELECT 2, post.id, profile.id, 'second comment'
  FROM
    (SELECT id FROM post LIMIT 1) AS post,
    (SELECT id FROM profile WHERE name = 'SecondUser') AS profile;
