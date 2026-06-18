CREATE TABLE _otp_code_with_no_case_email (
  id                           INTEGER PRIMARY KEY NOT NULL,
  user                         BLOB NOT NULL,
  email                        TEXT NOT NULL COLLATE NOCASE,
  otp_code                     TEXT NOT NULL,
  created                      INTEGER DEFAULT (UNIXEPOCH()) NOT NULL,
  expires                      INTEGER  NOT NULL
) STRICT;

INSERT INTO _otp_code_with_no_case_email (
    id,
    user,
    email,
    otp_code,
    created,
    expires
  )
  SELECT
    id,
    user,
    email,
    otp_code,
    created,
    expires
  FROM _otp_code;

DROP TABLE _otp_code;
ALTER TABLE _otp_code_with_no_case_email RENAME TO _otp_code;

-- Main OTP-code lookup.
CREATE UNIQUE INDEX __otp_code__email ON _otp_code (email);
