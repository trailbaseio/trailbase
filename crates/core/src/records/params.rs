use log::*;
use std::collections::HashSet;
use std::sync::Arc;
use trailbase_schema::json::{flat_json_to_value, json_array_to_bytes};
use trailbase_schema::sqlite::{Column, ColumnDataType};
use trailbase_schema::{FileUpload, FileUploadInput, FileUploads};
use trailbase_sqlite::{NamedParams, Value};
use trailbase_sqlvalue::SqlValue;

use crate::records::RecordApi;
use crate::schema_metadata::{self, JsonColumnMetadata, TableMetadata};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParamsError {
  #[error("Not a Number")]
  NotANumber,
  #[error("Column error: {0}")]
  Column(&'static str),
  #[error("Value not found")]
  ValueNotFound,
  #[error("Unexpected type: {0}, expected {1}")]
  UnexpectedType(&'static str, String),
  #[error("Decoding error: {0}")]
  Base64Decode(base64::DecodeError),
  #[error("Nested object: {0}")]
  NestedObject(String),
  #[error("Nested array: {0}")]
  NestedArray(String),
  #[error("Parse int error: {0}")]
  ParseInt(#[from] std::num::ParseIntError),
  #[error("Parse float error: {0}")]
  ParseFloat(#[from] std::num::ParseFloatError),
  #[error("Json validation error: {0}")]
  JsonValidation(#[from] schema_metadata::JsonSchemaError),
  #[error("Json serialization error: {0}")]
  JsonSerialization(Arc<serde_json::Error>),
  #[error("Json schema error: {0}")]
  Schema(#[from] trailbase_schema::Error),
  #[error("ObjectStore error: {0}")]
  Storage(Arc<object_store::Error>),
  #[error("SqlValueDecode: {0}")]
  SqlValueDecode(#[from] trailbase_sqlvalue::DecodeError),
}

impl From<serde_json::Error> for ParamsError {
  fn from(err: serde_json::Error) -> Self {
    return Self::JsonSerialization(Arc::new(err));
  }
}

impl From<object_store::Error> for ParamsError {
  fn from(err: object_store::Error) -> Self {
    return Self::Storage(Arc::new(err));
  }
}

impl From<trailbase_schema::json::JsonError> for ParamsError {
  fn from(value: trailbase_schema::json::JsonError) -> Self {
    return match value {
      trailbase_schema::json::JsonError::Finite => Self::NotANumber,
      trailbase_schema::json::JsonError::ValueNotFound => Self::ValueNotFound,
      trailbase_schema::json::JsonError::NotSupported => Self::UnexpectedType("?", "?".to_string()),
      trailbase_schema::json::JsonError::Decode(err) => Self::Base64Decode(err),
      trailbase_schema::json::JsonError::UnexpectedType(expected, got) => {
        Self::UnexpectedType(expected, format!("{got:?}"))
      }
      trailbase_schema::json::JsonError::ParseInt(err) => Self::ParseInt(err),
      trailbase_schema::json::JsonError::ParseFloat(err) => Self::ParseFloat(err),
    };
  }
}

// Contains Metadata (i.e. column contents) and file contents.
pub(crate) type FileMetadataContents = Vec<(FileUpload, Vec<u8>)>;

pub(crate) type JsonRow = serde_json::Map<String, serde_json::Value>;

pub trait SchemaAccessor {
  fn column_by_name(
    &self,
    field_name: &str,
  ) -> Option<(usize, &Column, Option<&JsonColumnMetadata>)>;
}

/// Implementation to build insert/update Params for admin APIs.
impl SchemaAccessor for TableMetadata {
  #[inline]
  fn column_by_name(
    &self,
    field_name: &str,
  ) -> Option<(usize, &Column, Option<&JsonColumnMetadata>)> {
    return self
      .column_by_name(field_name)
      .map(|(index, col)| (index, col, self.json_metadata.columns[index].as_ref()));
  }
}

/// Implementation to build insert/update Params for record APIs.
impl SchemaAccessor for RecordApi {
  #[inline]
  fn column_by_name(
    &self,
    field_name: &str,
  ) -> Option<(usize, &Column, Option<&JsonColumnMetadata>)> {
    return self.column_index_by_name(field_name).map(|index| {
      return (
        index,
        &self.columns()[index],
        self.json_column_metadata()[index].as_ref(),
      );
    });
  }
}

/// Represents a record provided by the user via request, i.e. a create or update record request.
///
/// To construct a `Params`, the request will be transformed, i.e. fields for unknown columns will
/// be filtered out and the json values will be translated into SQLite values.
pub enum Params {
  Insert {
    /// List of named params with their respective placeholders, e.g.:
    ///   '(":col_name": Value::Text("hi"))'.
    named_params: NamedParams,

    /// List of files and contents to be written to an object store.
    files: FileMetadataContents,

    /// Metadata for mapping `named_params` back to SQL schema to construct Insert/Update queries.
    column_names: Vec<String>,
    column_indexes: Vec<usize>,
  },
  Update {
    /// List of named params with their respective placeholders, e.g.:
    ///   '(":col_name": Value::Text("hi"))'.
    named_params: NamedParams,

    /// List of files and contents to be written to an object store.
    files: FileMetadataContents,

    /// Metadata for mapping `named_params` back to SQL schema to construct Insert/Update queries.
    column_names: Vec<String>,
    column_indexes: Vec<usize>,

    pk_column_name: String,
  },
}

impl Params {
  /// Converts a Json object + optional MultiPart files into trailbase_sqlite::Values and extracted
  /// files.
  pub fn for_insert<S: SchemaAccessor>(
    accessor: &S,
    json: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(json.len());
    let mut column_names = Vec::with_capacity(json.len());
    let mut column_indexes = Vec::with_capacity(json.len());

    let mut files: FileMetadataContents = vec![];

    // Insert parameters case.
    for (key, value) in json {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some((index, col, json_meta)) = accessor.column_by_name(&key) else {
        continue;
      };

      let (param, json_files) = extract_params_and_files_from_json(col, json_meta, value)?;

      if let Some(json_files) = json_files {
        // Note: files provided as a multipart form upload are handled below. They need more
        // special handling to establish the field.name to column mapping.
        files.extend(json_files);
      }

      named_params.push((prefix_colon(&key).into(), param));
      column_names.push(key);
      column_indexes.push(index);
    }

    // Note: files provided as part of a JSON request are handled above.
    if let Some(multipart_files) = multipart_files {
      files.extend(extract_files_from_multipart(
        accessor,
        multipart_files,
        &mut named_params,
        &mut column_names,
        &mut column_indexes,
      )?);
    }

    return Ok(Params::Insert {
      named_params,
      files,
      column_names,
      column_indexes,
    });
  }

  pub fn for_admin_insert<S: SchemaAccessor>(
    accessor: &S,
    row: indexmap::IndexMap<String, SqlValue>,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(row.len());
    let mut column_names = Vec::with_capacity(row.len());
    let mut column_indexes = Vec::with_capacity(row.len());

    // Insert parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some((index, _col, _json_meta)) = accessor.column_by_name(&key) else {
        continue;
      };

      named_params.push((prefix_colon(&key).into(), value.try_into()?));
      column_names.push(key);
      column_indexes.push(index);
    }

    return Ok(Params::Insert {
      named_params,
      files: vec![],
      column_names,
      column_indexes,
    });
  }

  pub fn for_update<S: SchemaAccessor>(
    accessor: &S,
    json: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    pk_column_name: String,
    pk_column_value: Value,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(json.len() + 1);
    let mut column_names = Vec::with_capacity(json.len() + 1);
    let mut column_indexes = Vec::with_capacity(json.len() + 1);

    let mut files: FileMetadataContents = vec![];

    // Update parameters case.
    for (key, value) in json {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some((index, col, json_meta)) = accessor.column_by_name(&key) else {
        continue;
      };

      let (param, json_files) = extract_params_and_files_from_json(col, json_meta, value)?;
      if let Some(json_files) = json_files {
        // Note: files provided as a multipart form upload are handled below. They need more
        // special handling to establish the field.name to column mapping.
        files.extend(json_files);
      }

      if key == pk_column_name && pk_column_value != param {
        return Err(ParamsError::Column(
          "Primary key mismatch in update request",
        ));
      }

      named_params.push((prefix_colon(&key).into(), param));
      column_names.push(key);
      column_indexes.push(index);
    }

    // Inject the pk_value. It may already be present, if redundantly provided both in the API path
    // *and* the request. In most cases it probably wont and duplication is not an issue.
    named_params.push((":__pk_value".into(), pk_column_value));

    // Note: files provided as part of a JSON request are handled above.
    if let Some(multipart_files) = multipart_files {
      files.extend(extract_files_from_multipart(
        accessor,
        multipart_files,
        &mut named_params,
        &mut column_names,
        &mut column_indexes,
      )?);
    }

    return Ok(Params::Update {
      named_params,
      files,
      column_names,
      column_indexes,
      pk_column_name,
    });
  }

  pub fn for_admin_update<S: SchemaAccessor>(
    accessor: &S,
    row: indexmap::IndexMap<String, SqlValue>,
    pk_column_name: String,
    pk_column_value: SqlValue,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(row.len() + 1);
    let mut column_names = Vec::with_capacity(row.len() + 1);
    let mut column_indexes = Vec::with_capacity(row.len() + 1);

    let pk_column_param = pk_column_value.try_into()?;

    // Update parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some((index, _col, _json_meta)) = accessor.column_by_name(&key) else {
        continue;
      };

      let param: Value = value.try_into()?;

      if key == pk_column_name && pk_column_param != param {
        return Err(ParamsError::Column(
          "Primary key mismatch in update request",
        ));
      }

      named_params.push((prefix_colon(&key).into(), param));
      column_names.push(key);
      column_indexes.push(index);
    }

    // Inject the pk_value. It may already be present, if redundantly provided both in the API path
    // *and* the request. In most cases it probably wont and duplication is not an issue.
    named_params.push((":__pk_value".into(), pk_column_param));

    return Ok(Params::Update {
      named_params,
      files: vec![],
      column_names,
      column_indexes,
      pk_column_name,
    });
  }
}

/// A lazy representation of SQL query parameters derived from the request json to share between
/// handler and the policy engine.
///
/// If the request gets rejected by the policy we want to avoid parsing the request JSON and if the
/// engine requires a parse we don't want to re-parse in the handler.
pub enum LazyParams<'a> {
  LazyInsert(Option<Box<dyn (FnOnce() -> Result<Params, ParamsError>) + Send + 'a>>),
  LazyUpdate(Option<Box<dyn (FnOnce() -> Result<Params, ParamsError>) + Send + 'a>>),
  Params(Result<Params, ParamsError>),
}

impl<'a> LazyParams<'a> {
  pub fn for_insert<S: SchemaAccessor + Sync>(
    accessor: &'a S,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Self {
    return LazyParams::LazyInsert(Some(Box::new(move || {
      return Params::for_insert(accessor, json_row, multipart_files);
    })));
  }

  pub fn for_update<S: SchemaAccessor + Sync>(
    accessor: &'a S,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    primary_key_column: String,
    primary_key_value: Value,
  ) -> Self {
    return LazyParams::LazyUpdate(Some(Box::new(move || {
      return Params::for_update(
        accessor,
        json_row,
        multipart_files,
        primary_key_column,
        primary_key_value,
      );
    })));
  }

  pub fn params(&mut self) -> Result<&'_ Params, ParamsError> {
    return match self {
      LazyParams::Params(result) => result.as_ref().map_err(|err| err.clone()),
      LazyParams::LazyInsert(builder) | LazyParams::LazyUpdate(builder) => {
        let Some(builder) = std::mem::take(builder) else {
          unreachable!("empty builder");
        };

        *self = Self::Params(builder());
        let LazyParams::Params(result) = self else {
          unreachable!("just assigned");
        };
        result.as_ref().map_err(|err| err.clone())
      }
    };
  }

  pub fn consume(self) -> Result<Params, ParamsError> {
    return match self {
      LazyParams::Params(result) => result,
      LazyParams::LazyInsert(builder) | LazyParams::LazyUpdate(builder) => match builder {
        Some(f) => f(),
        None => unreachable!("missing builder"),
      },
    };
  }
}

fn extract_files_from_multipart<S: SchemaAccessor>(
  accessor: &S,
  multipart_files: Vec<FileUploadInput>,
  named_params: &mut NamedParams,
  column_names: &mut Vec<String>,
  column_indexes: &mut Vec<usize>,
) -> Result<FileMetadataContents, ParamsError> {
  let files: Vec<(String, FileUpload, Vec<u8>)> = multipart_files
    .into_iter()
    .map(|file| {
      let (col_name, file_metadata, content) = file.consume()?;
      let Some(col_name) = col_name else {
        return Err(ParamsError::Column(
          "Multipart form upload missing name property",
        ));
      };
      return Ok((col_name, file_metadata, content));
    })
    .collect::<Result<_, ParamsError>>()?;

  // Validate and organize by type;
  let mut uploaded_files = HashSet::<&'static str>::new();
  for (field_name, file_metadata, _content) in &files {
    // We simply skip unknown columns, this could simply be malformed input or version skew. This
    // is similar in spirit to protobuf's unknown fields behavior.
    let Some((index, col, json_meta)) = accessor.column_by_name(field_name) else {
      continue;
    };

    let Some(JsonColumnMetadata::SchemaName(schema_name)) = &json_meta else {
      return Err(ParamsError::Column("Expected json column"));
    };

    let value = Value::Text(serde_json::to_string(&file_metadata)?);
    match schema_name.as_str() {
      "std.FileUpload" => {
        if !uploaded_files.insert(&col.name) {
          return Err(ParamsError::Column(
            "Collision: too many files for std.FileUpload",
          ));
        }

        named_params.push((prefix_colon(&col.name).into(), value));
        column_names.push(col.name.to_string());
        column_indexes.push(index);
      }
      "std.FileUploads" => {
        named_params.push((prefix_colon(&col.name).into(), value));
        column_names.push(col.name.to_string());
        column_indexes.push(index);
      }
      _ => {
        return Err(ParamsError::Column("Mismatching JSON schema"));
      }
    }
  }

  return Ok(
    files
      .into_iter()
      .map(|(_, file_metadata, content)| (file_metadata, content))
      .collect(),
  );
}

fn extract_params_and_files_from_json(
  col: &Column,
  json_meta: Option<&JsonColumnMetadata>,
  value: serde_json::Value,
) -> Result<(Value, Option<FileMetadataContents>), ParamsError> {
  return match value {
    serde_json::Value::Object(ref _map) => {
      // Only text columns are allowed to store nested JSON as text.
      if col.data_type != ColumnDataType::Text {
        return Err(ParamsError::NestedObject(format!(
          "Column data mismatch for: {col:?}",
        )));
      }

      let Some(ref json) = json_meta else {
        return Err(ParamsError::NestedObject(format!(
          "Missing JSON metadata for: {col:?}",
        )));
      };

      // By default, nested json will be serialized to text since that's what sqlite expected.
      // For FileUpload columns we have special handling to extract the actual payload and
      // convert the FileUploadInput into an actual FileUpload schema json.
      match json {
        JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
          let file_upload: FileUploadInput = serde_json::from_value(value)?;

          let (_col_name, metadata, content) = file_upload.consume()?;
          let param = Value::Text(serde_json::to_string(&metadata)?);

          Ok((param, Some(vec![(metadata, content)])))
        }
        _ => {
          json.validate(&value)?;
          Ok((Value::Text(value.to_string()), None))
        }
      }
    }
    serde_json::Value::Array(ref arr) => {
      // If the we're building a Param for a schema column, unpack the json (and potentially files)
      // and validate it.
      match col.data_type {
        ColumnDataType::Blob => return Ok((Value::Blob(json_array_to_bytes(arr)?), None)),
        ColumnDataType::Text => {
          match json_meta {
            Some(JsonColumnMetadata::SchemaName(name)) if name == "std.FileUploads" => {
              let file_upload_vec: Vec<FileUploadInput> = serde_json::from_value(value)?;

              // TODO: Optimize the copying here. Not very critical.
              let mut temp: Vec<FileUpload> = vec![];
              let mut uploads: FileMetadataContents = vec![];
              for file in file_upload_vec {
                let (_col_name, metadata, content) = file.consume()?;
                temp.push(metadata.clone());
                uploads.push((metadata, content));
              }

              let param = Value::Text(serde_json::to_string(&FileUploads(temp))?);

              return Ok((param, Some(uploads)));
            }
            Some(schema) => {
              schema.validate(&value)?;
              return Ok((Value::Text(value.to_string()), None));
            }
            _ => {}
          }
        }
        _ => {}
      }

      Err(ParamsError::NestedArray(format!(
        "Received nested array for column: {col:?}",
      )))
    }
    x => Ok((flat_json_to_value(col.data_type, x)?, None)),
  };
}

#[inline]
pub(crate) fn prefix_colon(s: &str) -> String {
  let mut new = String::with_capacity(s.len() + 1);
  new.push(':');
  new.push_str(s);
  return new;
}

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use schemars::{JsonSchema, schema_for};
  use serde_json::json;
  use trailbase_schema::parse::parse_into_statement;
  use trailbase_schema::sqlite::Table;

  use super::*;
  use crate::constants::USER_TABLE;
  use crate::records::test_utils::json_row_from_value;
  use crate::schema_metadata::TableMetadata;
  use crate::util::id_to_b64;

  #[tokio::test]
  async fn test_params() {
    #[allow(unused)]
    #[derive(JsonSchema)]
    struct TestSchema {
      text: String,
      array: Option<Vec<serde_json::Value>>,
      blob: Option<Vec<u8>>,
    }

    const SCHEMA_NAME: &str = "test.TestSchema";
    let schema = schema_for!(TestSchema);
    const ID_COL: &str = "myid";
    const ID_COL_PLACEHOLDER: &str = ":myid";

    let sql = format!(
      r#"
          CREATE TABLE user (
            {ID_COL} BLOB NOT NULL,
            blob BLOB NOT NULL,
            text TEXT NOT NULL,
            json_col TEXT NOT NULL CHECK(jsonschema('{SCHEMA_NAME}', json_col)),
            num INTEGER NOT NULL DEFAULT 42,
            real REAL NOT NULL DEFAULT 23.0
          )
    "#
    );

    let table: Table = parse_into_statement(&sql)
      .unwrap()
      .unwrap()
      .try_into()
      .unwrap();

    trailbase_schema::registry::set_user_schema(
      SCHEMA_NAME,
      Some(serde_json::to_value(schema).unwrap()),
    )
    .unwrap();
    trailbase_extension::jsonschema::get_schema(SCHEMA_NAME).unwrap();

    let metadata = TableMetadata::new(table.clone(), &[table], USER_TABLE);

    let id: [u8; 16] = uuid::Uuid::now_v7().as_bytes().clone();
    let blob: Vec<u8> = [0; 128].to_vec();
    let text = "some text :)";
    let num = 5;
    let real = 3.0;

    let assert_params = |p: &Params| {
      let Params::Insert {
        named_params,
        files: _,
        column_names: _,
        column_indexes: _,
      } = p
      else {
        panic!("Not an insert")
      };

      assert!(named_params.len() >= 5, "{:?}", named_params);

      for (param, value) in named_params {
        match param.as_ref() {
          ID_COL_PLACEHOLDER => {
            assert!(
              matches!(value, Value::Blob(x) if *x == id),
              "VALUE: {value:?}"
            );
          }
          ":blob" => {
            assert!(matches!(value, Value::Blob(x) if *x == blob));
          }
          ":text" => {
            assert!(matches!(value, Value::Text(x) if x.contains("some text :)")));
          }
          ":num" => {
            assert!(matches!(value, Value::Integer(x) if *x == 5));
          }
          ":real" => {
            assert!(matches!(value, Value::Real(x) if *x == 3.0));
          }
          ":json_col" => {
            assert!(matches!(value, Value::Text(_x)));
          }
          x => assert!(false, "{x}"),
        }
      }
    };

    {
      // Test that blob columns can be passed as base64.
      let value = json!({
        ID_COL: id_to_b64(&id),
        "blob": BASE64_URL_SAFE.encode(&blob),
        "text": text,
        "num": num,
        "real": real,
      });

      assert_params(
        &Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).unwrap(),
      );
    }

    {
      // Test that blob columns can be passed as int array and numbers can be passed as string.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "num": 5,
        "real": 3,
      });

      assert_params(
        &Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).unwrap(),
      );

      // Make sure we're strictly parsing, e.g. not converting strings to numbers.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "num": "5",
        "real": "3",
      });

      assert!(
        Params::for_insert(&metadata, json_row_from_value(value.clone()).unwrap(), None).is_err()
      );
    }

    {
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": json!({
          "email": text,
        }),
        "num": 5,
        "real": 3,
      });

      assert!(Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).is_err());

      // Test that nested JSON object can be passed.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "json_col": json!({
          "text": text,
        }),
        "num": 5,
        "real": 3,
      });

      let params =
        Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).unwrap();
      assert_params(&params);
    }

    {
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": json!([text, 1,2,3,4, "foo"]),
        "num": 5,
        "real": 3,
      });

      assert!(Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).is_err());

      // Test that nested JSON array can be passed.
      let nested_json_blob: Vec<u8> = vec![65, 66, 67, 68];
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "json_col": json!({
          "text": "test",
          "array": [text, 1,2,3,4, "foo"],
          "blob": nested_json_blob,
        }),
        "num": 5,
        "real": 3,
      });

      let params =
        Params::for_insert(&metadata, json_row_from_value(value).unwrap(), None).unwrap();

      assert_params(&params);

      let Params::Insert { named_params, .. } = params else {
        panic!("Not an insert");
      };
      let json_col: Vec<Value> = named_params
        .iter()
        .filter_map(|(name, value)| {
          if name == ":json_col" {
            return Some(value.clone());
          }
          return None;
        })
        .collect();

      assert_eq!(json_col.len(), 1);
      let Value::Text(ref text) = json_col[0] else {
        panic!("Unexpected param type: {:?}", json_col[0]);
      };

      // Test encoded nested json against golden.
      assert_eq!(
        serde_json::from_str::<serde_json::Value>(text).unwrap(),
        serde_json::json!({
          "array": Vec::<serde_json::Value>::from(["some text :)".into(),1.into(),2.into(),3.into(),4.into(),"foo".into()]),
          "blob": [65,66,67,68],
          "text": "test",
        }),
      );
    }
  }
}
