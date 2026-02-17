use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;
use trailbase_schema::json::flat_json_to_value;
use trailbase_schema::metadata::ColumnMetadata;
use trailbase_schema::registry::JsonSchemaRegistry;
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
  #[error("Geos: {0}")]
  Geos(#[from] geos::Error),
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

pub trait ColumnAccessor {
  fn column_by_name(&self, field_name: &str) -> Option<&ColumnMetadata>;
}

/// Implementation to build insert/update Params for admin APIs.
impl ColumnAccessor for TableMetadata {
  #[inline]
  fn column_by_name(&self, field_name: &str) -> Option<&ColumnMetadata> {
    return self.column_by_name(field_name);
  }
}

/// Implementation to build insert/update Params for record APIs.
impl ColumnAccessor for RecordApi {
  #[inline]
  fn column_by_name(&self, field_name: &str) -> Option<&ColumnMetadata> {
    return self.column_metadata_by_name(field_name);
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
  pub fn for_insert<S: ColumnAccessor>(
    accessor: &S,
    json_schema_registry: &JsonSchemaRegistry,
    row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(row.len());
    let mut column_names = Vec::with_capacity(row.len());
    let mut column_indexes = Vec::with_capacity(row.len());

    let mut files: FileMetadataContents = vec![];

    // Insert parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some(ColumnMetadata {
        index,
        column,
        json,
        is_file: _,
        is_geometry,
      }) = accessor.column_by_name(&key)
      else {
        continue;
      };

      let (param, json_files) = extract_params_and_files_from_json(
        json_schema_registry,
        column,
        json.as_ref(),
        *is_geometry,
        value,
      )?;
      if let Some(json_files) = json_files {
        // Note: files provided as a multipart form upload are handled below. They need more
        // special handling to establish the field.name to column mapping.
        files.extend(json_files);
      }

      named_params.push((prefix_colon(&key).into(), param));
      column_names.push(key);
      column_indexes.push(*index);
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

  pub fn for_admin_insert<S: ColumnAccessor>(
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
      let Some(ColumnMetadata {
        index,
        column: _,
        json: _,
        is_file: _,
        is_geometry: _,
      }) = accessor.column_by_name(&key)
      else {
        continue;
      };

      named_params.push((prefix_colon(&key).into(), value.try_into()?));
      column_names.push(key);
      column_indexes.push(*index);
    }

    return Ok(Params::Insert {
      named_params,
      files: vec![],
      column_names,
      column_indexes,
    });
  }

  pub fn for_update<S: ColumnAccessor>(
    accessor: &S,
    json_schema_registry: &JsonSchemaRegistry,
    row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    pk_column_name: String,
    pk_column_value: Value,
  ) -> Result<Self, ParamsError> {
    let mut named_params = NamedParams::with_capacity(row.len() + 1);
    let mut column_names = Vec::with_capacity(row.len() + 1);
    let mut column_indexes = Vec::with_capacity(row.len() + 1);

    let mut files: FileMetadataContents = vec![];

    // Update parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or version skew. This
      // is similar in spirit to protobuf's unknown fields behavior.
      let Some(ColumnMetadata {
        index,
        column,
        json,
        is_file: _,
        is_geometry,
      }) = accessor.column_by_name(&key)
      else {
        continue;
      };

      let (param, json_files) = extract_params_and_files_from_json(
        json_schema_registry,
        column,
        json.as_ref(),
        *is_geometry,
        value,
      )?;
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
      column_indexes.push(*index);
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

  pub fn for_admin_update<S: ColumnAccessor>(
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
      let Some(ColumnMetadata {
        index,
        column: _,
        json: _,
        is_file: _,
        is_geometry: _,
      }) = accessor.column_by_name(&key)
      else {
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
      column_indexes.push(*index);
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
  pub fn for_insert<S: ColumnAccessor + Sync>(
    accessor: &'a S,
    json_schema_registry: Arc<RwLock<JsonSchemaRegistry>>,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Self {
    return LazyParams::LazyInsert(Some(Box::new(move || {
      return Params::for_insert(
        accessor,
        &json_schema_registry.read(),
        json_row,
        multipart_files,
      );
    })));
  }

  pub fn for_update<S: ColumnAccessor + Sync>(
    accessor: &'a S,
    json_schema_registry: Arc<RwLock<JsonSchemaRegistry>>,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    primary_key_column: String,
    primary_key_value: Value,
  ) -> Self {
    return LazyParams::LazyUpdate(Some(Box::new(move || {
      return Params::for_update(
        accessor,
        &json_schema_registry.read(),
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

fn extract_files_from_multipart<S: ColumnAccessor>(
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
    let Some(ColumnMetadata {
      index,
      column,
      json,
      is_file: _,
      is_geometry: _,
    }) = accessor.column_by_name(field_name)
    else {
      continue;
    };

    let Some(JsonColumnMetadata::SchemaName(schema_name)) = &json else {
      return Err(ParamsError::Column("Expected json file column"));
    };

    match schema_name.as_str() {
      "std.FileUpload" => {
        if !uploaded_files.insert(&column.name) {
          return Err(ParamsError::Column(
            "Collision: too many files for std.FileUpload",
          ));
        }

        named_params.push((
          prefix_colon(&column.name).into(),
          Value::Text(serde_json::to_string(&file_metadata)?),
        ));
        column_names.push(column.name.to_string());
        column_indexes.push(*index);
      }
      "std.FileUploads" => {
        named_params.push((
          prefix_colon(&column.name).into(),
          Value::Text(serde_json::to_string(&file_metadata)?),
        ));
        column_names.push(column.name.to_string());
        column_indexes.push(*index);
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
  json_schema_registry: &JsonSchemaRegistry,
  col: &Column,
  json_metadata: Option<&JsonColumnMetadata>,
  is_geometry: bool,
  value: serde_json::Value,
) -> Result<(Value, Option<FileMetadataContents>), ParamsError> {
  // If this is *not* a JSON column convert the value trivially.
  let Some(json_metadata) = json_metadata else {
    if is_geometry && col.data_type == ColumnDataType::Blob {
      use geos::Geom;

      let json_geometry = geos::geojson::Geometry::from_json_value(value)
        .map_err(|err| ParamsError::UnexpectedType("", format!("GeoJSON: {err}")))?;
      let geometry: geos::Geometry = json_geometry.try_into()?;

      let mut writer = geos::WKBWriter::new()?;
      if geometry.get_srid().is_ok() {
        writer.set_include_SRID(true);
      }

      return Ok((Value::Blob(writer.write_wkb(&geometry)?.into()), None));
    }

    debug_assert!(!is_geometry);

    return Ok((flat_json_to_value(col.data_type, value)?, None));
  };

  // So this *is* a JSON column => we need to be smarter.
  if col.data_type != ColumnDataType::Text {
    return Err(ParamsError::NestedObject(format!(
      "Column data mismatch for: {col:?}",
    )));
  }

  // Handle file columns specially, i.e. convert the JSON.
  match json_metadata {
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUpload" => {
      let file_upload: FileUploadInput = serde_json::from_value(value)?;

      let (_col_name, metadata, content) = file_upload.consume()?;
      let param = Value::Text(serde_json::to_string(&metadata)?);

      return Ok((param, Some(vec![(metadata, content)])));
    }
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
      let file_upload_vec: Vec<FileUploadInput> = serde_json::from_value(value)?;

      let uploads: FileMetadataContents = file_upload_vec
        .into_iter()
        .map(|file| {
          let (_col_name, metadata, content) = file.consume()?;
          return Ok((metadata, content));
        })
        .collect::<Result<Vec<_>, ParamsError>>()?;

      let param = Value::Text(serde_json::to_string(&FileUploads(
        uploads
          .iter()
          .map(|(metadata, _content)| metadata.clone())
          .collect(),
      ))?);

      return Ok((param, Some(uploads)));
    }
    _ => {}
  }

  // NOTE: We're doing early validation here for JSON inputs. This leads to redudant double
  // validation down the line. We could also *not* do it and leave it to the SQLite `jsonschema`
  // extension function, however this may help to reduce SQLite congestion for invalid inputs.
  return match value {
    serde_json::Value::String(s) => {
      // WARN: It's completely unclear if we should allow passing JSON objects as string in a
      // request. We just used to accept it. In theory, accepting a string when the JSON
      // schema expects an object is a loop-hole. We may want to remove this in the future
      // :shrug:.
      let json_value: serde_json::Value = serde_json::from_str(&s)
        .map_err(|err| ParamsError::NestedObject(format!("invalid json: {err}")))?;

      json_metadata.validate(json_schema_registry, &json_value)?;
      Ok((Value::Text(s), None))
    }
    value => {
      json_metadata.validate(json_schema_registry, &value)?;
      Ok((Value::Text(value.to_string()), None))
    }
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

    let registry = trailbase_schema::registry::build_json_schema_registry(vec![(
      SCHEMA_NAME.to_string(),
      serde_json::to_value(&schema_for!(TestSchema)).unwrap(),
    )])
    .unwrap();

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

    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();

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
        &Params::for_insert(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None,
        )
        .unwrap(),
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
        &Params::for_insert(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None,
        )
        .unwrap(),
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
        Params::for_insert(
          &metadata,
          &registry,
          json_row_from_value(value.clone()).unwrap(),
          None
        )
        .is_err()
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

      assert!(
        Params::for_insert(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None
        )
        .is_err()
      );

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

      let params = Params::for_insert(
        &metadata,
        &registry,
        json_row_from_value(value).unwrap(),
        None,
      )
      .unwrap();
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

      assert!(
        Params::for_insert(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None
        )
        .is_err()
      );

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

      let params = Params::for_insert(
        &metadata,
        &registry,
        json_row_from_value(value).unwrap(),
        None,
      )
      .unwrap();

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
