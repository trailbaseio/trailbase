INSERT INTO _user
  (id, email, password_hash, verified, admin)
VALUES
  (uuid_v7(), 'admin@localhost', (hash_password('secret')), TRUE, TRUE);
