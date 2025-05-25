#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

pub mod error;
pub mod file;
pub mod json_schema;
pub mod metadata;
pub mod registry;
pub mod sqlite;

pub use error::Error;
pub use file::{FileUpload, FileUploadInput, FileUploads};
pub use sqlite::QualifiedName;

pub struct QualifiedNameEscaped(String);

impl QualifiedNameEscaped {
  pub fn new(name: &QualifiedName) -> Self {
    return Self(name.escaped_string());
  }
}

impl From<QualifiedName> for QualifiedNameEscaped {
  fn from(name: QualifiedName) -> Self {
    return Self::new(&name);
  }
}

impl From<&QualifiedName> for QualifiedNameEscaped {
  fn from(name: &QualifiedName) -> Self {
    return Self::new(name);
  }
}

impl std::fmt::Display for QualifiedNameEscaped {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return self.0.fmt(f);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_qualified_name() {
    assert!(QualifiedName::parse(r#"test"; DROP TABLE students;""#).is_err());

    let simple = QualifiedName::parse("test").unwrap();
    assert_eq!(
      simple,
      QualifiedName {
        name: "test".to_string(),
        database_schema: None,
      }
    );
    assert_eq!(QualifiedNameEscaped::new(&simple).0, r#""test""#);

    let composite = QualifiedName::parse("db.test.bar").unwrap();
    assert_eq!(
      composite,
      QualifiedName {
        name: "test.bar".to_string(),
        database_schema: Some("db".to_string()),
      }
    );
    assert_eq!(
      QualifiedNameEscaped::new(&composite).0,
      r#""db"."test.bar""#
    );

    let unescape = QualifiedName::parse("[db].'test'").unwrap();
    assert_eq!(
      unescape,
      QualifiedName {
        name: "test".to_string(),
        database_schema: Some("db".to_string()),
      }
    );
  }
}
