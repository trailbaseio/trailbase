-- Stop using `email` column as scratch space for yet unverified emails. This
-- way we guarantee that `email` may only contain validated entries. Otherwise,
-- logged in users may appear with unverified emails until their auth token
-- expires.
CREATE EXTENSION IF NOT EXISTS citext;

ALTER TABLE _user ADD COLUMN unverified_email CITEXT;

UPDATE _user SET
  email = NULL,
  unverified_email = email
WHERE
  verified = FALSE;

ALTER TABLE _user DROP COLUMN verified;
