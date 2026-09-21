INSERT INTO _user
  (id, email, username, password_hash, admin)
VALUES
  (uuid_v7(), 'admin@localhost', 'admin', (hash_password('secret')), TRUE);
