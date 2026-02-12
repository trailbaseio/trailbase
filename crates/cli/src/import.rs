use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use trailbase::api::cli::ImportUser;

#[derive(Debug, Deserialize, Serialize)]
struct Auth0User {
  email_verified: bool,
  email: String,
  #[serde(rename = "passwordHash")]
  password_hash: String,
  password_set_date: Auth0Date,
  tenant: Option<String>,
  connection: Option<String>,
}

#[allow(unused)]
#[derive(Debug, Deserialize, Serialize)]
struct Auth0Id {
  #[serde(rename = "$oid")]
  oid: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Auth0Date {
  #[serde(rename = "$date")]
  date: DateTime<Utc>,
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Parse newline-deliminated JSON from Auth0.
pub fn read_auth0_nd_json(contents: &str) -> Result<Vec<ImportUser>, BoxError> {
  fn parse(line: &str) -> Result<Auth0User, BoxError> {
    let user = serde_json::from_str::<Auth0User>(line)?;

    // Check hash.
    if trailbase_extension::password::valid_hash(&user.password_hash) {
      return Ok(user);
    }
    return Err("invalid hash".into());
  }

  return contents
    .split("\n")
    .flat_map(|line| -> Option<Result<ImportUser, BoxError>> {
      if line.is_empty() {
        return None;
      }
      let auth0_user = match parse(line) {
        Ok(user) => user,
        Err(err) => {
          return Some(Err(err));
        }
      };

      return Some(Ok(ImportUser {
        email: auth0_user.email,
        password_hash: auth0_user.password_hash,
        verified: auth0_user.email_verified,
      }));
    })
    .collect();
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_auth0_import() {
    let users = read_auth0_nd_json(AUTH0_FILE).unwrap();
    assert_eq!(3, users.len());
    assert_eq!(
      ImportUser {
        email: "admin@localhost".to_string(),
        password_hash: "$2b$12$OziW5BRZpnl8FDOkOzqcxe/SFfq3n0sClAQHA6UnfT2Hl.mvtDDOi".to_string(),
        verified: true,
      },
      users[0]
    );
  }

  const AUTH0_FILE: &str = r#"
{"_ID":{"$oid":"60425da93519d90068f82966"},"email_verified":true,"email":"admin@localhost","passwordHash":"$2b$12$OziW5BRZpnl8FDOkOzqcxe/SFfq3n0sClAQHA6UnfT2Hl.mvtDDOi","password_set_date":{"$date":"2021-03-05T16:34:49.502Z"},"tenant":"dev-rwsbs6ym","connection":"Username-Password-Authentication","_tmp_is_unique":true}
{"_ID":{"$oid":"60425dc43519d90068f82973"},"email_verified":false,"email":"john@example.com","passwordHash":"$2b$10$Z6hUTEEeoJXN5/AmSm/4.eZ75RYgFVriQM9LPhNEC7kbAbS/VAaJ2","password_set_date":{"$date":"2021-03-05T16:35:16.775Z"},"tenant":"dev-rwsbs6ym","connection":"Username-Password-Authentication","_tmp_is_unique":true}
{"_ID":{"$oid":"60425da93519d90068f82968"},"email_verified":false,"email":"bob@example.com","passwordHash":"$2b$10$CSZ2JarG4XYbGa.JkfpqnO2wrlbfp5eb5LScHSGo9XGeZ.a.Ic54S","password_set_date":{"$date":"2021-03-05T16:34:49.502Z"},"tenant":"dev-rwsbs6ym","connection":"Username-Password-Authentication","_tmp_is_unique":true}

"#;
}
