use base64::prelude::*;
use log::*;
use std::collections::HashSet;
use std::sync::Arc;
use trailbase_schema::sqlite::{Column, ColumnDataType};
use trailbase_schema::{FileUpload, FileUploadInput, FileUploads};
use trailbase_sqlite::{NamedParams, Value};

use crate::records::RecordApi;
use crate::schema_metadata::{self, JsonColumnMetadata, TableMetadata};

#[derive(Debug, Clone, thiserror::Error)]
pub enum ParamsError {
  #[error("Not an object")]
  NotAnObject,
  #[error("Not a Number")]
  NotANumber,
  #[error("Column error: {0}")]
  Column(&'static str),
  #[error("Unexpected type: {0}, expected {1}")]
  UnexpectedType(&'static str, String),
  #[error("Decoding error: {0}")]
  Decode(#[from] base64::DecodeError),
  #[error("Nested object: {0}")]
  NestedObject(String),
  #[error("Nested array: {0}")]
  NestedArray(String),
  #[error("Inhomogenous array: {0}")]
  InhomogenousArray(String),
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
pub struct Params {
  /// List of named params with their respective placeholders, e.g.:
  ///   '(":col_name": Value::Text("hi"))'.
  pub(super) named_params: NamedParams,

  /// List of files and contents to be written to an object store.
  pub(super) files: FileMetadataContents,

  /// Metadata for mapping `named_params` back to SQL schema to construct Insert/Update queries.
  pub(super) column_names: Vec<String>,
  pub(super) column_indexes: Vec<usize>,
}

impl Params {
  /// Converts a top-level Json object into trailbase_sqlite::Values and extract files.
  ///
  /// Note: that this function by design is non-recursive, since we're mapping to a flat hierarchy
  /// in sqlite, since even JSON/JSONB is simply text/blob that is lazily parsed.
  ///
  /// The expected format is:
  ///
  /// request = {
  ///   "col0": "text",
  ///   "col1": <base64(b"123")>,
  ///   "file_col": {
  ///     data: ...
  ///   }
  /// }
  ///
  /// The optional files parameter is there to receive files in case the input JSON was extracted
  /// form a multipart/form. In that case files are handled separately and not embedded in the JSON
  /// value itself in contrast to when the original request was an actual JSON request.
  pub fn from<S: SchemaAccessor>(
    accessor: &S,
    json: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Result<Self, ParamsError> {
    let len = json.len();
    let mut params = Params {
      named_params: NamedParams::with_capacity(len),
      files: FileMetadataContents::default(),
      column_names: Vec::with_capacity(len),
      column_indexes: Vec::with_capacity(len),
    };

    for (key, value) in json {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some((index, col, json_meta)) = accessor.column_by_name(&key) else {
        continue;
      };

      let (param, mut json_files) = extract_params_and_files_from_json(col, json_meta, value)?;
      if let Some(json_files) = json_files.as_mut() {
        // Note: files provided as a multipart form upload are handled below. They need more
        // special handling to establish the field.name to column mapping.
        params.files.append(json_files);
      }

      params.named_params.push((prefix_colon(&key).into(), param));
      params.column_names.push(key);
      params.column_indexes.push(index);
    }

    // Note: files provided as part of a JSON request are handled above.
    if let Some(multipart_files) = multipart_files {
      params.append_multipart_files(accessor, multipart_files)?;
    }

    return Ok(params);
  }

  fn append_multipart_files<S: SchemaAccessor>(
    &mut self,
    accessor: &S,
    multipart_files: Vec<FileUploadInput>,
  ) -> Result<(), ParamsError> {
    let files: Vec<(String, FileUpload, Vec<u8>)> = multipart_files
      .into_iter()
      .map(|file| {
        let (col_name, file_metadata, content) = file.consume()?;
        return match col_name {
          Some(col_name) => Ok((col_name, file_metadata, content)),
          None => Err(ParamsError::Column(
            "Multipart form upload missing name property",
          )),
        };
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

          self
            .named_params
            .push((prefix_colon(&col.name).into(), value));
          self.column_names.push(col.name.to_string());
          self.column_indexes.push(index);
        }
        "std.FileUploads" => {
          self
            .named_params
            .push((prefix_colon(&col.name).into(), value));
          self.column_names.push(col.name.to_string());
          self.column_indexes.push(index);
        }
        _ => {
          return Err(ParamsError::Column("Mismatching JSON schema"));
        }
      }
    }

    self.files.append(
      &mut files
        .into_iter()
        .map(|(_, file_metadata, content)| (file_metadata, content))
        .collect(),
    );

    return Ok(());
  }
}

/// A lazy representation of SQL query parameters derived from the request json to share between
/// handler and the policy engine.
///
/// If the request gets rejected by the policy we want to avoid parsing the request JSON and if the
/// engine requires a parse we don't want to re-parse in the handler.
///
/// NOTE: Table level access checking could probably happen even sooner before we process multipart
/// streams at all.
pub struct LazyParams<'a, S: SchemaAccessor> {
  // Input
  accessor: &'a S,
  json_row: JsonRow,
  multipart_files: Option<Vec<FileUploadInput>>,

  // Cached evaluate params. We could use a OnceCell but we don't need the synchronisation.
  result: Option<Result<Params, ParamsError>>,
}

impl<'a, S: SchemaAccessor> LazyParams<'a, S> {
  pub fn new(
    accessor: &'a S,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Self {
    LazyParams {
      accessor,
      json_row,
      multipart_files,
      result: None,
    }
  }

  pub fn params(&mut self) -> Result<&'_ Params, ParamsError> {
    let result = self.result.get_or_insert_with(|| {
      Params::from(
        self.accessor,
        std::mem::take(&mut self.json_row),
        std::mem::take(&mut self.multipart_files),
      )
    });

    return result.as_ref().map_err(|err| err.clone());
  }

  pub fn consume(mut self) -> Result<Params, ParamsError> {
    return self
      .result
      .take()
      .unwrap_or_else(|| Params::from(self.accessor, self.json_row, self.multipart_files));
  }
}

fn extract_params_and_files_from_json(
  col: &Column,
  json_meta: Option<&JsonColumnMetadata>,
  value: serde_json::Value,
) -> Result<(Value, Option<FileMetadataContents>), ParamsError> {
  let col_name = &col.name;
  match value {
    serde_json::Value::Object(ref _map) => {
      // Only text columns are allowed to store nested JSON as text.
      if col.data_type != ColumnDataType::Text {
        return Err(ParamsError::NestedObject(format!(
          "Column data mismatch for: {col_name}"
        )));
      }

      let Some(ref json) = json_meta else {
        return Err(ParamsError::NestedObject(format!(
          "Plain text column w/o JSON: {col_name}"
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

          return Ok((param, Some(vec![(metadata, content)])));
        }
        _ => {
          json.validate(&value)?;
          return Ok((Value::Text(value.to_string()), None));
        }
      }
    }
    serde_json::Value::Array(ref arr) => {
      // If the we're building a Param for a schema column, unpack the json (and potentially files)
      // and validate it.
      match col.data_type {
        ColumnDataType::Blob => return Ok((try_json_array_to_blob(arr)?, None)),
        ColumnDataType::Text => {
          if let Some(ref json) = json_meta {
            match json {
              JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
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
              schema => {
                schema.validate(&value)?;
                return Ok((Value::Text(value.to_string()), None));
              }
            }
          }
        }
        _ => {}
      }

      return Err(ParamsError::NestedArray(format!(
        "Received nested array for unsuitable column: {col_name}"
      )));
    }
    x => return Ok((simple_json_value_to_param(col.data_type, x)?, None)),
  };
}

pub fn simple_json_value_to_param(
  col_type: ColumnDataType,
  value: serde_json::Value,
) -> Result<Value, ParamsError> {
  let param = match value {
    serde_json::Value::Object(ref _map) => {
      return Err(ParamsError::UnexpectedType(
        "Object",
        format!("Trivial type: {col_type:?}"),
      ));
    }
    serde_json::Value::Array(ref arr) => {
      // NOTE: Convert Array<number> to Blob. Note, we also support blobs as base64 which are
      // handled below in the string  case.
      if col_type != ColumnDataType::Blob {
        return Err(ParamsError::UnexpectedType(
          "Array",
          format!("Trivial type: {col_type:?}"),
        ));
      }

      try_json_array_to_blob(arr)?
    }
    serde_json::Value::Null => Value::Null,
    serde_json::Value::Bool(b) => {
      if col_type != ColumnDataType::Integer {
        return Err(ParamsError::UnexpectedType("Bool", format!("{col_type:?}")));
      }
      Value::Integer(b as i64)
    }
    serde_json::Value::String(str) => json_string_to_value(col_type, str)?,
    serde_json::Value::Number(number) => {
      if let Some(n) = number.as_i64() {
        match col_type {
          ColumnDataType::Integer => Value::Integer(n),
          // NOTE: "as" is lossy conversion. Does not panic.
          ColumnDataType::Real => Value::Real(n as f64),
          _ => {
            return Err(ParamsError::UnexpectedType("int", format!("{col_type:?}")));
          }
        }
      } else if let Some(n) = number.as_u64() {
        match col_type {
          // NOTE: "as" is lossy conversion. Does not panic.
          ColumnDataType::Integer => Value::Integer(n as i64),
          ColumnDataType::Real => Value::Real(n as f64),
          _ => {
            return Err(ParamsError::UnexpectedType("uint", format!("{col_type:?}")));
          }
        }
      } else if let Some(n) = number.as_f64() {
        match col_type {
          ColumnDataType::Real => Value::Real(n),
          _ => {
            return Err(ParamsError::UnexpectedType("real", format!("{col_type:?}")));
          }
        }
      } else {
        warn!("Not a valid number: {number:?}");
        return Err(ParamsError::NotANumber);
      }
    }
  };

  return Ok(param);
}

fn try_json_array_to_blob(arr: &Vec<serde_json::Value>) -> Result<Value, ParamsError> {
  let mut byte_array: Vec<u8> = vec![];
  for el in arr {
    match el {
      serde_json::Value::Number(num) => {
        let Some(int) = num.as_i64() else {
          return Err(ParamsError::UnexpectedType(
            "NonByteNumber",
            format!("Number type: {num:?}"),
          ));
        };

        let Ok(byte) = int.try_into() else {
          return Err(ParamsError::UnexpectedType(
            "NonByteNumber",
            format!("Out-of-range int: {int}"),
          ));
        };

        byte_array.push(byte);
      }
      x => {
        return Err(ParamsError::InhomogenousArray(format!(
          "Expected number, got {x:?}"
        )));
      }
    };
  }

  return Ok(Value::Blob(byte_array));
}

pub(crate) fn json_string_to_value(
  data_type: ColumnDataType,
  value: String,
) -> Result<Value, ParamsError> {
  return Ok(match data_type {
    ColumnDataType::Null => Value::Null,
    // Strict/storage types
    ColumnDataType::Any => Value::Text(value),
    ColumnDataType::Text => Value::Text(value),
    ColumnDataType::Blob => Value::Blob(match (value.len(), value) {
      // Special handling for text encoded UUIDs. Right now we're guessing based on length, it
      // would be more explicit rely on CHECK(...) column options.
      // NOTE: That uuids also parse as url-safe base64, that's why we treat it as a fall-first.
      (36, v) => uuid::Uuid::parse_str(&v)
        .map(|v| v.into())
        .or_else(|_| BASE64_URL_SAFE.decode(&v))?,
      (_, v) => BASE64_URL_SAFE.decode(&v)?,
    }),
    ColumnDataType::Integer => Value::Integer(value.parse::<i64>()?),
    ColumnDataType::Real => Value::Real(value.parse::<f64>()?),
    ColumnDataType::Numeric => Value::Integer(value.parse::<i64>()?),
    // JSON types.
    ColumnDataType::JSONB => Value::Blob(value.into_bytes().to_vec()),
    ColumnDataType::JSON => Value::Text(value),
    // Affine types
    //
    // Integers:
    ColumnDataType::Int
    | ColumnDataType::TinyInt
    | ColumnDataType::SmallInt
    | ColumnDataType::MediumInt
    | ColumnDataType::BigInt
    | ColumnDataType::UnignedBigInt
    | ColumnDataType::Int2
    | ColumnDataType::Int4
    | ColumnDataType::Int8 => Value::Integer(value.parse::<i64>()?),
    // Text:
    ColumnDataType::Character
    | ColumnDataType::Varchar
    | ColumnDataType::VaryingCharacter
    | ColumnDataType::NChar
    | ColumnDataType::NativeCharacter
    | ColumnDataType::NVarChar
    | ColumnDataType::Clob => Value::Text(value),
    // Real:
    ColumnDataType::Double | ColumnDataType::DoublePrecision | ColumnDataType::Float => {
      Value::Real(value.parse::<f64>()?)
    }
    // Numeric
    ColumnDataType::Boolean
    | ColumnDataType::Decimal
    | ColumnDataType::Date
    | ColumnDataType::DateTime => Value::Integer(value.parse::<i64>()?),
  });
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
  async fn test_json_string_to_value() {
    let conn = trailbase_sqlite::Connection::open_in_memory().unwrap();
    conn
      .execute(
        r#"CREATE TABLE test (
        id    BLOB NOT NULL,
        text  TEXT
    )"#,
        (),
      )
      .await
      .unwrap();

    let id_string = "01950408-de17-7f13-8ef5-66d90b890bfd";
    let id = uuid::Uuid::parse_str(id_string).unwrap();

    conn
      .execute(
        "INSERT INTO test (id, text) VALUES ($1, $2);",
        trailbase_sqlite::params!(id.into_bytes(), "mytext",),
      )
      .await
      .unwrap();

    let value = json_string_to_value(ColumnDataType::Blob, id_string.to_string()).unwrap();
    let blob = match value {
      rusqlite::types::Value::Blob(ref blob) => blob.clone(),
      _ => panic!("Not a blob"),
    };

    assert_eq!(
      blob.len(),
      16,
      "Blob: {value:?} {}",
      String::from_utf8_lossy(&blob)
    );
    assert_eq!(uuid::Uuid::from_slice(&blob).unwrap(), id);

    let rows = conn
      .read_query_rows("SELECT * FROM test WHERE id = $1", [value])
      .await
      .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String>(1).unwrap(), "mytext");
  }

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

    let assert_params = |p: Params| {
      assert!(p.named_params.len() >= 5, "{:?}", p.named_params);

      for (param, value) in &p.named_params {
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

      assert_params(Params::from(&metadata, json_row_from_value(value).unwrap(), None).unwrap());
    }

    {
      // Test that blob columns can be passed as int array and numbers can be passed as string.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "num": "5",
        "real": "3",
      });

      assert_params(Params::from(&metadata, json_row_from_value(value).unwrap(), None).unwrap());
    }

    {
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": json!({
          "email": text,
        }),
        "num": "5",
        "real": "3",
      });

      assert!(Params::from(&metadata, json_row_from_value(value).unwrap(), None).is_err());

      // Test that nested JSON object can be passed.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "json_col": json!({
          "text": text,
        }),
        "num": "5",
        "real": "3",
      });

      let params = Params::from(&metadata, json_row_from_value(value).unwrap(), None).unwrap();
      assert_params(params);
    }

    {
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": json!([text, 1,2,3,4, "foo"]),
        "num": "5",
        "real": "3",
      });

      assert!(Params::from(&metadata, json_row_from_value(value).unwrap(), None).is_err());

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
        "num": "5",
        "real": "3",
      });

      let params = Params::from(&metadata, json_row_from_value(value).unwrap(), None).unwrap();

      let json_col: Vec<Value> = params
        .named_params
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

      assert_params(params);
    }
  }
}
