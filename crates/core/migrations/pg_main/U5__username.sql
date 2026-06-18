-- Add `username` column and make emails/password_hash nullable.
CREATE EXTENSION IF NOT EXISTS citext;

ALTER TABLE _user ALTER COLUMN email DROP NOT NULL;
ALTER TABLE _user ALTER COLUMN email TYPE CITEXT;
ALTER TABLE _user ALTER COLUMN email DROP NOT NULL;

ALTER TABLE _user ADD COLUMN username CITEXT;

ALTER TABLE _user ALTER COLUMN password_hash DROP NOT NULL;
ALTER TABLE _user ALTER COLUMN password_hash SET DEFAULT NULL;
