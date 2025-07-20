-- Create default admin user with top "secret" password.
INSERT INTO _user
  (email, password_hash, verified, admin)
VALUES
  ('admin@localhost', (hash_password('secret')), TRUE, TRUE);
