-- Crate a unique constraint for unverified_email addresses too to catch
-- redundant registrations early and avoid ambiguous lingering users with
-- the same unverified email address.

-- Do a quick cleanup to reduce likelihood of pre-existing violations. We also
-- started to do this cleanup scheduled periodically.
DELETE FROM _user WHERE
  unverified_email IS NOT NULL AND
  UNIXEPOCH() > (created + 24 * 3600);

CREATE UNIQUE INDEX __user__unverified_email_index ON _user (unverified_email);
