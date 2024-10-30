-- Create admin user with "secret" password.
INSERT INTO _user
  (email, password_hash, verified, admin)
VALUES
  ('admin@localhost', (hash_password('secret')), TRUE, TRUE);

-- Set a username for the admin user.
INSERT INTO profiles (user, username)
  SELECT user.id, 'Admin'
  FROM _user AS user WHERE email = 'admin@localhost';
