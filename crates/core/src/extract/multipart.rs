//! Parse multipart form requests
use axum::{
  body::Body,
  extract::{FromRequest, Request},
};
use serde::de::DeserializeOwned;
use serde_json::json;
use thiserror::Error;
use trailbase_schema::FileUploadInput;

#[derive(Debug, Error)]
pub enum Rejection {
  #[error("Failed to read request body: {0}")]
  ReadBody(#[from] axum::Error),
  #[error("Failed to read multipart payload: {0}")]
  Multipart(#[from] axum::extract::multipart::MultipartRejection),
  #[error("Failed to deserialize Multipart: {0}")]
  MultipartField(#[from] axum::extract::multipart::MultipartError),
  #[error("Failed to deserialize JSON: {0}")]
  Serde(#[from] serde_path_to_error::Error<serde_json::Error>),
  #[error("Precondition error: {0}")]
  Precondition(&'static str),
}

/// Parse a multipart form submission into the specified type and a list of files uploaded with it.
///
/// Note, when encountering stream errors one should check the tower limit layers. The error is
/// pretty cryptic when the stream gets cut off.
pub async fn parse_multipart<T>(req: Request<Body>) -> Result<(T, Vec<FileUploadInput>), Rejection>
where
  T: DeserializeOwned + Send + Sync + 'static,
{
  let mut multipart = axum::extract::Multipart::from_request(req, &()).await?;

  let mut data = serde_json::Map::<String, serde_json::Value>::new();
  let mut files: Vec<FileUploadInput> = vec![];

  while let Some(mut field) = multipart.next_field().await? {
    if field.file_name().is_some() {
      // We

      let content_type = field.content_type().map(|s| s.to_string());
      let name = field.name().map(|s| s.to_string());
      let filename = field.file_name().map(|s| s.to_string());

      let mut buffer: Vec<u8> = vec![];
      while let Some(chunk) = field.chunk().await? {
        buffer.extend_from_slice(&chunk);
      }

      // Forms submit an empty string for optional file inputs :/.
      if buffer.is_empty() {
        continue;
      }

      files.push(FileUploadInput {
        name,
        filename,
        content_type,
        data: buffer,
      });
    } else if let Some(name) = field.name() {
      coerce_and_push_array(&mut data, name.to_string(), json!(field.text().await?));
    } else {
      // We consider form fields that neither have a filename nor a name to be invalid.
      return Err(Rejection::Precondition("Neither name nor filename"));
    }
  }

  return Ok((
    serde_path_to_error::deserialize(json!(data)).map_err(Rejection::Serde)?,
    files,
  ));
}

/// Adds ([key], [value]) to [map], first as value and subsequently as an array, i.e.
///   `map[key]=[v0, v1, ...]`.
fn coerce_and_push_array(
  map: &mut serde_json::Map<String, serde_json::Value>,
  key: String,
  value: serde_json::Value,
) {
  return match map.get_mut(&key) {
    Some(serde_json::Value::Array(a)) => {
      a.push(value);
    }
    Some(v) => {
      let old = v.take();
      *v = json!([old, value]);
    }
    None => {
      map.insert(key, value);
    }
  };
}

#[cfg(test)]
mod test {
  use super::*;
  use indoc::indoc;

  fn get_req() -> axum::http::Request<axum::body::Body> {
    let body = indoc! {r#"
        --fieldB
        Content-Disposition: form-data; name="name"

        test
        --fieldB
        Content-Disposition: form-data; name="file1"; filename="a.txt"
        Content-Type: text/plain

        Some text
        --fieldB
        Content-Disposition: form-data; name="file2"; filename="a.html"
        Content-Type: text/html

        <b>Some html</b>
        --fieldB
        Content-Disposition: form-data; name="agreed"

        on
        --fieldB--
        "#}
    .replace("\n", "\r\n");

    axum::http::Request::builder()
      .header("content-type", "multipart/form-data; boundary=fieldB")
      .header("content-length", body.len())
      .body(axum::body::Body::from(body))
      .unwrap()
  }

  #[tokio::test]
  async fn parse_multipart_jsonvalue() {
    let data = get_req();
    let (value, files) = super::parse_multipart::<serde_json::Value>(data)
      .await
      .unwrap();
    assert_eq!(
      value,
      json!({
          "name": "test",
          "agreed": "on"
      })
    );

    assert_eq!(
      files,
      vec![
        (FileUploadInput {
          name: Some("file1".to_string()),
          filename: Some("a.txt".to_string()),
          content_type: Some("text/plain".to_string()),
          data: Vec::from("Some text".as_bytes())
        }),
        (FileUploadInput {
          name: Some("file2".to_string()),
          filename: Some("a.html".to_string()),
          content_type: Some("text/html".to_string()),
          data: Vec::from("<b>Some html</b>".as_bytes())
        }),
      ]
    );
  }
}
