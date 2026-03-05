-- Add a a few non-admin users.
INSERT INTO _user (id, email, password_hash, verified, admin, totp_secret)
VALUES
  (uuid_v4(), 'alice@trailbase.io', (hash_password('secret')), TRUE, TRUE, 'YCUTAYEZ346ZUEI7FLCG57BOMZQHHRA5'),
  (uuid_v4(), '0@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '1@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '2@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '3@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '4@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '5@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '6@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '7@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '8@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '9@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '10@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '11@localhost', (hash_password('secret')), TRUE, FALSE, NULL),
  (uuid_v4(), '12@localhost', (hash_password('secret')), TRUE, FALSE, NULL);
