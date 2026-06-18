INSERT INTO _user
  (id, email, username, password_hash, verified, admin)
VALUES
  (uuid_v4(), 'admin@localhost', 'admin', (hash_password('secret')), TRUE, TRUE);
