-- Add an optional tenant id to `_session` table for token refreshes.
ALTER TABLE _session ADD COLUMN tenant BLOB;

-- Add an optional tenant id to `_authorization_code` for PKCE logins.
ALTER TABLE _authorization_code ADD COLUMN tenant BLOB;
