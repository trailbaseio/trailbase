-- Add an editor
INSERT INTO _user (email, password_hash, verified) VALUES ('editor@localhost', (hash_password('secret')), TRUE);

-- Set a username for the editor user.
INSERT INTO profiles (user, username)
  SELECT user.id, 'EddyEditor'
  FROM _user AS user WHERE email = 'editor@localhost';

-- Add an avatar image for the editor user.
INSERT INTO _user_avatar (user, file)
  SELECT user.id, '{"id":"0328bc95-9622-42e7-a609-625769a797c2","filename":"admin.png","content_type":"image/png","mime_type":"image/png"}'
  FROM _user AS user WHERE email = 'editor@localhost';

-- Add the editor user to the editors group
INSERT INTO editors (user)
  SELECT user.id FROM _user AS user WHERE email = 'editor@localhost';


-- Add another user: non-admin, non-editor and w/o profile.
INSERT INTO _user (email, password_hash, verified) VALUES ('other@localhost', (hash_password('secret')), TRUE);
