-- Add a a few non-admin users.
INSERT INTO _user (id, email, password_hash, verified)
VALUES
  (uuid_v7(), '0@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '1@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '2@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '3@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '4@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '5@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '6@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '7@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '8@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '9@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '10@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '11@localhost', (hash_password('secret')), TRUE),
  (uuid_v7(), '12@localhost', (hash_password('secret')), TRUE);
