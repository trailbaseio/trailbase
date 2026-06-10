-- Add `handle` column and make emails/password_hash nullable.

ALTER TABLE _user ALTER COLUMN email DROP NOT NULL;

ALTER TABLE _user ADD COLUMN handle TEXT;

ALTER TABLE _user ALTER COLUMN password_hash DROP NOT NULL;
ALTER TABLE _user ALTER COLUMN password_hash SET DEFAULT NULL;
