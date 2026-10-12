use compact_str::CompactString;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use trailbase_schema::db::metadata::ColumnMetadata;
use trailbase_schema::db::sqlite::{Column, ColumnDataType};
use trailbase_schema::json::flat_json_to_value;
use trailbase_schema::json_schema::registry::JsonSchemaRegistry;
use trailbase_schema::json_schema::{FileUpload, FileUploadInput, FileUploads};
use trailbase_sqlite::Value;
use trailbase_sqlite::to_sql::ToSqlProxy;
use trailbase_sqlvalue::SqlValue;

use crate::records::RecordApi;
use crate::records::util::named_placeholder;
use crate::schema_metadata::{JsonColumnMetadata, TableMetadata};

mod error;

pub use error::ParamsError;

/// Contains Metadata (i.e. column contents) and file contents.
pub(crate) type FileMetadataContents = Vec<(FileUpload, Option<Vec<u8>>)>;

/// A JSON object representing a DB record/row.
pub(crate) type JsonRow = serde_json::Map<String, serde_json::Value>;

#[derive(Debug)]
pub struct NamedParam {
  pub column_index: usize,
  pub column_name: CompactString,
  pub placeholder: CompactString,
  pub value: Value,
}

pub trait ToParamsEnum {
  fn params_enum(&self) -> Option<Params>;
}

impl ToParamsEnum for Arc<()> {
  fn params_enum(&self) -> Option<Params> {
    return None;
  }
}

#[derive(Debug)]
pub struct InsertParams {
  /// List of named params with their respective placeholders, e.g.:
  pub params: Vec<NamedParam>,
}

impl ToParamsEnum for Arc<InsertParams> {
  fn params_enum(&self) -> Option<Params> {
    return Some(Params::Insert(self.clone()));
  }
}

pub struct ArcInsertParams(pub Arc<InsertParams>);

impl trailbase_sqlite::Params for ArcInsertParams {
  fn bind<S: trailbase_sqlite::Statement>(
    self,
    stmt: &mut S,
  ) -> Result<(), trailbase_sqlite::Error> {
    #[cfg(test)]
    assert_eq!(1, Arc::strong_count(&self.0));

    for NamedParam {
      placeholder, value, ..
    } in &self.0.params
    {
      if let Some(idx) = stmt.parameter_index(&placeholder)? {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(value.into()))?;
      };
    }

    return Ok(());
  }
}

impl InsertParams {
  /// Converts a Json object + optional MultiPart files into trailbase_sqlite::Values and extracted
  /// files.
  pub fn new<S: ColumnAccessor>(
    accessor: &S,
    json_schema_registry: &JsonSchemaRegistry,
    row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Result<(Self, FileMetadataContents), ParamsError> {
    let mut params = Vec::<NamedParam>::with_capacity(row.len());
    let mut files: FileMetadataContents = vec![];

    // Insert parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or client version
      // skew. This is similar in spirit to protobuf's unknown fields behavior.
      let Some(ColumnMetadata {
        index,
        column,
        json,
        is_geometry,
        ..
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

      params.push(NamedParam {
        column_index: *index,
        placeholder: named_placeholder(&key),
        column_name: key.into(),
        value: param,
      });
    }

    // Note: files provided as part of a JSON request are handled above.
    if let Some(multipart_files) = multipart_files {
      files.extend(extract_files_from_multipart(
        accessor,
        multipart_files,
        &mut params,
      )?);
    }

    return Ok((Self { params }, files));
  }

  pub fn for_admin_insert<S: ColumnAccessor>(
    accessor: &S,
    row: indexmap::IndexMap<String, SqlValue>,
  ) -> Result<Self, ParamsError> {
    let mut params = Vec::<NamedParam>::with_capacity(row.len());

    // Insert parameters case.
    for (key, value) in row {
      // For admin operations we don't skip. There should not be client version skew.
      let Some(ColumnMetadata { index, .. }) = accessor.column_by_name(&key) else {
        return Err(ParamsError::ColumnNotFound);
      };

      params.push(NamedParam {
        column_index: *index,
        placeholder: named_placeholder(&key),
        column_name: key.into(),
        value: value.try_into()?,
      });
    }

    return Ok(Self { params });
  }
}

#[derive(Debug)]
pub struct UpdateParams {
  /// List of named params with their respective placeholders, e.g.:
  pub params: Vec<NamedParam>,

  /// Primary key column name & value.
  pub pk_column_name: String,
  pub pk_column_value: Value,
}

impl ToParamsEnum for Arc<UpdateParams> {
  fn params_enum(&self) -> Option<Params> {
    return Some(Params::Update(self.clone()));
  }
}

pub struct ArcUpdateParams(pub Arc<UpdateParams>);

impl trailbase_sqlite::Params for ArcUpdateParams {
  fn bind<S: trailbase_sqlite::Statement>(
    self,
    stmt: &mut S,
  ) -> Result<(), trailbase_sqlite::Error> {
    #[cfg(test)]
    assert_eq!(1, Arc::strong_count(&self.0));

    for NamedParam {
      placeholder, value, ..
    } in &self.0.params
    {
      if let Some(idx) = stmt.parameter_index(&placeholder)? {
        stmt.bind_parameter(idx, ToSqlProxy::Borrowed(value.into()))?;
      };
    }

    if let Some(idx) = stmt.parameter_index(":__pk_value")? {
      stmt.bind_parameter(idx, ToSqlProxy::Borrowed((&self.0.pk_column_value).into()))?;
    } else {
      #[cfg(debug_assertions)]
      panic!(":__pk_value not found");
    }

    return Ok(());
  }
}

impl UpdateParams {
  pub fn new<S: ColumnAccessor>(
    accessor: &S,
    json_schema_registry: &JsonSchemaRegistry,
    row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    pk_column_name: String,
    pk_column_value: Value,
  ) -> Result<(Self, FileMetadataContents), ParamsError> {
    let mut params = Vec::<NamedParam>::with_capacity(row.len());
    let mut files: FileMetadataContents = vec![];

    // Update parameters case.
    for (key, value) in row {
      // We simply skip unknown columns, this could simply be malformed input or client version
      // skew. This is similar in spirit to protobuf's unknown fields behavior.
      let Some(ColumnMetadata {
        index,
        column,
        json,
        is_geometry,
        ..
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

      params.push(NamedParam {
        column_index: *index,
        placeholder: named_placeholder(&key),
        column_name: key.into(),
        value: param,
      });
    }

    // Make sure not everything was skipped, otherwise we'll yield an invalid query leading to
    // a server-error.
    if params.is_empty() {
      return Err(ParamsError::Column("no valid columns to update found"));
    }

    // Note: files provided as part of a JSON request are handled above.
    if let Some(multipart_files) = multipart_files {
      files.extend(extract_files_from_multipart(
        accessor,
        multipart_files,
        &mut params,
      )?);
    }

    return Ok((
      Self {
        params,
        pk_column_name,
        pk_column_value,
      },
      files,
    ));
  }

  pub fn for_admin_update<S: ColumnAccessor>(
    accessor: &S,
    json_schema_registry: Arc<JsonSchemaRegistry>,
    row: indexmap::IndexMap<String, SqlValue>,
    pk_column_name: String,
    pk_column_value: SqlValue,
  ) -> Result<(Self, FileMetadataContents), ParamsError> {
    let pk_column_param = pk_column_value.try_into()?;

    let mut params = Vec::<NamedParam>::with_capacity(row.len());
    let mut files: FileMetadataContents = vec![];

    // Update parameters case.
    for (key, value) in row {
      // For admin operations we don't skip. There should not be client version skew.
      let Some(ColumnMetadata {
        index,
        column,
        json,
        is_geometry,
        ..
      }) = accessor.column_by_name(&key)
      else {
        return Err(ParamsError::ColumnNotFound);
      };

      let param: Value = if let Some(JsonColumnMetadata::SchemaName(schema_name)) = json.as_ref() {
        match schema_name.as_str() {
          "std.FileUpload" | "std.FileUploads" => {
            match value {
              SqlValue::Text(text) => {
                let (param, json_files) = extract_params_and_files_from_json(
                  &json_schema_registry,
                  column,
                  json.as_ref(),
                  *is_geometry,
                  serde_json::from_str(&text)?,
                )?;
                if let Some(json_files) = json_files {
                  // Note: files provided as a multipart form upload are handled below. They need
                  // more special handling to establish the field.name to column
                  // mapping.
                  files.extend(json_files);
                }

                param
              }
              SqlValue::Null => Value::Null,
              _ => {
                return Err(ParamsError::UnexpectedType("", format!("{value:?}")));
              }
            }
          }
          _ => value.try_into()?,
        }
      } else {
        value.try_into()?
      };

      if key == pk_column_name && pk_column_param != param {
        return Err(ParamsError::Column(
          "Primary key mismatch in update request",
        ));
      }

      params.push(NamedParam {
        column_index: *index,
        placeholder: named_placeholder(&key),
        column_name: key.into(),
        value: param,
      });
    }

    // Make sure not everything was skipped, otherwise we'll yield an invalid query leading to
    // a server-error.
    if params.is_empty() {
      return Err(ParamsError::Column("no valid columns to update found"));
    }

    return Ok((
      Self {
        params,
        pk_column_name,
        pk_column_value: pk_column_param,
      },
      files,
    ));
  }
}

/// Represents a record provided by the user via request, i.e. a create or update record request.
///
/// To construct a `Params`, the request will be transformed, i.e. fields for unknown columns will
/// be filtered out and the json values will be translated into SQLite values.
#[derive(Debug)]
pub enum Params {
  Insert(Arc<InsertParams>),
  Update(Arc<UpdateParams>),
}

/// A lazy representation of SQL query parameters derived from the request json to share between
/// handler and the policy engine.
///
/// If the request gets rejected by the policy we want to avoid parsing the request JSON and if the
/// engine requires a parse we don't want to re-parse in the handler.
pub enum LazyParams<'a, T: 'static> {
  Builder(
    Option<Box<dyn (FnOnce() -> Result<(T, FileMetadataContents), ParamsError>) + Send + 'a>>,
  ),
  Params(Result<(Arc<T>, FileMetadataContents), ParamsError>),
}

impl<'a, T: 'static> LazyParams<'a, T> {
  pub fn params(&mut self) -> Result<&'_ (Arc<T>, FileMetadataContents), &ParamsError> {
    return match self {
      LazyParams::Params(result) => result.as_ref(),
      LazyParams::Builder(builder) => {
        let Some(builder) = std::mem::take(builder) else {
          unreachable!("empty builder");
        };

        *self = Self::Params(builder().map(|(p, f)| (Arc::new(p), f)));
        let LazyParams::Params(result) = self else {
          unreachable!("just assigned");
        };
        result.as_ref()
      }
    };
  }

  pub fn consume(self) -> Result<(Arc<T>, FileMetadataContents), ParamsError> {
    return match self {
      LazyParams::Params(result) => result,
      LazyParams::Builder(builder) => match builder {
        Some(f) => f().map(|(p, f)| (Arc::new(p), f)),
        None => unreachable!("missing builder"),
      },
    };
  }
}

impl<'a> LazyParams<'a, InsertParams> {
  pub fn for_insert<S: ColumnAccessor + Sync>(
    accessor: &'a S,
    json_schema_registry: Arc<JsonSchemaRegistry>,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
  ) -> Self {
    return LazyParams::Builder(Some(Box::new(move || {
      return InsertParams::new(accessor, &json_schema_registry, json_row, multipart_files);
    })));
  }
}

impl<'a> LazyParams<'a, UpdateParams> {
  pub fn for_update<S: ColumnAccessor + Sync>(
    accessor: &'a S,
    json_schema_registry: Arc<JsonSchemaRegistry>,
    json_row: JsonRow,
    multipart_files: Option<Vec<FileUploadInput>>,
    primary_key_column: String,
    primary_key_value: Value,
  ) -> Self {
    return LazyParams::Builder(Some(Box::new(move || {
      return UpdateParams::new(
        accessor,
        &json_schema_registry,
        json_row,
        multipart_files,
        primary_key_column,
        primary_key_value,
      );
    })));
  }
}

fn extract_files_from_multipart<S: ColumnAccessor>(
  accessor: &S,
  multipart_files: Vec<FileUploadInput>,
  params: &mut Vec<NamedParam>,
) -> Result<FileMetadataContents, ParamsError> {
  let files: Vec<(String, FileUpload, Option<Vec<u8>>)> = multipart_files
    .into_iter()
    .map(|file| {
      let (col_name, file_metadata, content) = file.consume()?;
      let Some(col_name) = col_name else {
        return Err(ParamsError::Column(
          "Multipart form upload missing name property",
        ));
      };
      return Ok((col_name, file_metadata, Some(content.0)));
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
      ..
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

        params.push(NamedParam {
          column_index: *index,
          placeholder: named_placeholder(&column.name),
          column_name: CompactString::from(&column.name),
          value: Value::Text(serde_json::to_string(&file_metadata)?),
        });
      }
      "std.FileUploads" => {
        params.push(NamedParam {
          column_index: *index,
          placeholder: named_placeholder(&column.name),
          column_name: CompactString::from(&column.name),
          value: Value::Text(serde_json::to_string(&file_metadata)?),
        });
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
  #[allow(unused)] is_geometry: bool,
  value: serde_json::Value,
) -> Result<(Value, Option<FileMetadataContents>), ParamsError> {
  // If this is *not* a JSON column convert the value trivially.
  let Some(json_metadata) = json_metadata else {
    #[cfg(any(feature = "geos", feature = "geos-static"))]
    if is_geometry && col.data_type == ColumnDataType::Blob {
      use geos::Geom;

      let json_geometry = geos::geojson::Geometry::from_json_value(value)
        .map_err(|err| ParamsError::UnexpectedType("", format!("GeoJSON: {err}")))?;
      let geometry: geos::Geometry = json_geometry.try_into()?;

      let mut writer = geos::WKBWriter::new()?;
      if geometry.get_srid().is_ok() {
        writer.set_include_SRID(true);
      }

      return Ok((Into::<Value>::into(writer.write_wkb(&geometry)?), None));
    } else {
      debug_assert!(!is_geometry);
    }

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
      let (metadata, contents) = extract_param_and_file_from_json_value(value)?;
      let param = Value::Text(serde_json::to_string(&metadata)?);
      return Ok((param, Some(vec![(metadata, contents)])));
    }
    JsonColumnMetadata::SchemaName(name) if name == "std.FileUploads" => {
      let serde_json::Value::Array(array) = value else {
        return Err(ParamsError::UnexpectedType("array", format!("{value:?}")));
      };

      let uploads: FileMetadataContents = array
        .into_iter()
        .map(|value| {
          let (metadata, contents) = extract_param_and_file_from_json_value(value)?;
          return Ok((metadata, contents));
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

fn extract_param_and_file_from_json_value(
  value: serde_json::Value,
) -> Result<(FileUpload, Option<Vec<u8>>), ParamsError> {
  // We allow inputs to either be "fresh" inputs containing actual data bytes or metadata
  // round-tripped from prior reads (where the data is already stored). The latter is useful for
  // updates and partial deletions.
  //
  // IMPORTANT: order matters. Input needs to be first to make Metadata the fallback.
  #[derive(Debug, Deserialize)]
  #[serde(untagged)]
  enum InputOrMetadata {
    Input(FileUploadInput),
    Metadata(FileUpload),
  }

  return match serde_json::from_value::<InputOrMetadata>(value)? {
    InputOrMetadata::Input(file_upload_input) => {
      let (_col_name, metadata, content) = file_upload_input.consume()?;
      Ok((metadata, Some(content.0)))
    }
    InputOrMetadata::Metadata(metadata) => Ok((metadata, None)),
  };
}

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

#[cfg(test)]
mod tests {
  use base64::prelude::*;
  use schemars::{JsonSchema, schema_for};
  use serde_json::json;
  use trailbase_schema::db::parse_sql::{Bump, parse_into_statement};
  use trailbase_schema::db::sqlite::Table;
  use trailbase_schema::db::{QualifiedName, QualifiedNameEscaped};

  use super::*;
  use crate::config::proto;
  use crate::records::test_utils::json_row_from_value;
  use crate::records::write_queries::WriteQuery;
  use crate::schema_metadata::TableMetadata;
  use crate::util::id_to_b64;

  #[test]
  fn test_empty_insert_params() {
    let table_name = "test_table".to_string();

    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(
      &allocator,
      &format!(
        "CREATE TABLE {table_name} (
          id      INTEGER PRIMARY KEY,
          data    TEXT
        ) STRICT;"
      ),
    )
    .unwrap()
    .unwrap())
      .try_into()
      .unwrap();

    let registry =
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap();
    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();
    let qualified_name = QualifiedNameEscaped::new(&QualifiedName {
      name: table_name.clone(),
      database_schema: None,
    });

    struct Input {
      input: serde_json::Value,
      expected_params: usize,
    }

    for i in [
      Input {
        input: json!({}),
        expected_params: 0,
      },
      Input {
        input: json!({"unknown_column": 5}),
        expected_params: 0,
      },
      Input {
        input: json!({"data": "test"}),
        expected_params: 1,
      },
      Input {
        input: json!({"id": 5, "data": "test"}),
        expected_params: 2,
      },
    ] {
      let (params, _files) = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(i.input).unwrap(),
        None,
      )
      .unwrap();

      let query = WriteQuery::new_insert_or_replace(
        trailbase_sqlite::ConnectionType::Sqlite,
        &qualified_name,
        &metadata.column_metadata,
        "id",
        proto::ConflictResolutionStrategy::Replace,
        Arc::new(params),
      )
      .unwrap();

      let WriteQuery::Insert { query, params } = query else {
        panic!("not an insert");
      };

      assert_eq!(i.expected_params, params.params.len());

      // Make sure it's a valid query
      assert!(parse_into_statement(&allocator, &query).is_ok(), "{query}")
    }
  }

  #[test]
  fn test_non_trivial_insert_params() {
    #[allow(unused)]
    #[derive(JsonSchema)]
    struct TestSchema {
      text: String,
      array: Option<Vec<serde_json::Value>>,
      blob: Option<Vec<u8>>,
    }

    const SCHEMA_NAME: &str = "test.TestSchema";

    let registry = trailbase_schema::json_schema::registry::build_json_schema_registry(vec![(
      SCHEMA_NAME.to_string(),
      serde_json::to_value(&schema_for!(TestSchema)).unwrap(),
    )])
    .unwrap();

    const ID_COL: &str = "myid";

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

    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(&allocator, &sql).unwrap().unwrap())
      .try_into()
      .unwrap();

    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();

    let id: [u8; 16] = uuid::Uuid::now_v7().as_bytes().clone();
    let blob: Vec<u8> = [0; 128].to_vec();
    let text = "some text :)";
    let num = 5;
    let real = 3.0;

    let assert_params = |p: &InsertParams| {
      let InsertParams { params, .. } = p;
      assert!(params.len() >= 5, "{params:?}");

      for NamedParam {
        column_name, value, ..
      } in params
      {
        match column_name.as_str() {
          ID_COL => {
            assert!(
              matches!(value, Value::Blob(x) if x.to_vec() == id),
              "VALUE: {value:?}"
            );
          }
          "blob" => {
            assert!(matches!(value, Value::Blob(x) if x.to_vec() == blob));
          }
          "text" => {
            assert!(matches!(value, Value::Text(x) if x.contains("some text :)")));
          }
          "num" => {
            assert!(matches!(value, Value::Integer(x) if *x == 5));
          }
          "real" => {
            assert!(matches!(value, Value::Real(x) if *x == 3.0));
          }
          "json_col" => {
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
        &InsertParams::new(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None,
        )
        .unwrap()
        .0,
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
        &InsertParams::new(
          &metadata,
          &registry,
          json_row_from_value(value).unwrap(),
          None,
        )
        .unwrap()
        .0,
      );

      // Make sure we're strictly parsing, e.g. not converting strings to numbers.
      let value = json!({
        ID_COL: id,
        "blob": blob,
        "text": text,
        "num": "5",
        "real": "3",
      });

      let params = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(value.clone()).unwrap(),
        None,
      );

      assert!(params.is_err(), "{params:?}");
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
        InsertParams::new(
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

      let params = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(value).unwrap(),
        None,
      )
      .unwrap()
      .0;
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
        InsertParams::new(
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

      let params = InsertParams::new(
        &metadata,
        &registry,
        json_row_from_value(value).unwrap(),
        None,
      )
      .unwrap()
      .0;

      assert_params(&params);

      let json_col: Vec<Value> = params
        .params
        .iter()
        .filter_map(
          |NamedParam {
             placeholder, value, ..
           }| {
            if placeholder == ":json_col" {
              return Some(value.clone());
            }
            return None;
          },
        )
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

  #[test]
  fn test_empty_update_yields_err() {
    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(
      &allocator,
      "
      CREATE TABLE records (
        id      INTEGER PRIMARY KEY,
        data    TEXT
      ) STRICT;
      ",
    )
    .unwrap()
    .unwrap())
      .try_into()
      .unwrap();

    let registry =
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap();
    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();

    let id: i64 = 5;

    let empty = json!({});
    assert!(
      UpdateParams::new(
        &metadata,
        &registry,
        json_row_from_value(empty).unwrap(),
        None,
        "id".to_string(),
        Value::Integer(id),
      )
      .is_err()
    );

    let unknown_column = json!({
        "unknown_column": 5,
    });
    assert!(
      UpdateParams::new(
        &metadata,
        &registry,
        json_row_from_value(unknown_column).unwrap(),
        None,
        "id".to_string(),
        Value::Integer(id),
      )
      .is_err()
    );
  }

  #[test]
  fn test_for_update_skips_stored_file_upload() {
    let allocator = Bump::new();
    let table: Table = (&parse_into_statement(
      &allocator,
      "
      CREATE TABLE records (
        id      INTEGER PRIMARY KEY,
        name    TEXT NOT NULL,
        avatar  TEXT CHECK(jsonschema('std.FileUpload', avatar))
      ) STRICT;
      ",
    )
    .unwrap()
    .unwrap())
      .try_into()
      .unwrap();

    let registry =
      trailbase_schema::json_schema::registry::build_json_schema_registry(vec![]).unwrap();
    let metadata = TableMetadata::new(&registry, table.clone(), &[table]).unwrap();

    let id: i64 = 5;

    {
      let value = json!({
        "name": "Alice",
        "avatar": {
          "filename": "avatar_abc123.jpg",
          "original_filename": "avatar.jpg",
          "content_type": "image/jpeg",
          "mime_type": "image/jpeg"
        }
      });

      let (params, files) = UpdateParams::new(
        &metadata,
        &registry,
        json_row_from_value(value).unwrap(),
        None,
        "id".to_string(),
        Value::Integer(id),
      )
      .unwrap();

      let UpdateParams { params, .. } = params;

      // Avatar file-column should not be skipped but the file data is absent.
      assert_eq!(
        vec![":avatar", ":name"],
        params
          .iter()
          .map(|p| p.placeholder.as_str())
          .collect::<Vec<_>>()
      );
      assert_eq!(1, files.len());
      assert!(files[0].1.is_none());
    }

    // Test: FileUploadInput with 'data' field should NOT be skipped.
    {
      let png_data = BASE64_URL_SAFE.encode([137u8, 80, 78, 71]);
      let value_with_new_file = json!({
        "name": "Bob",
        "avatar": {
          "filename": "new_avatar.jpg",
          "content_type": "image/jpeg",
          "data": png_data
        }
      });

      let (params, files) = UpdateParams::new(
        &metadata,
        &registry,
        json_row_from_value(value_with_new_file).unwrap(),
        None,
        "id".to_string(),
        Value::Integer(id),
      )
      .unwrap();

      let UpdateParams { params, .. } = params;

      // Avatar should be present when there IS a 'data' field (new upload).
      assert_eq!(
        vec![":avatar", ":name"],
        params
          .iter()
          .map(|p| p.placeholder.as_str())
          .collect::<Vec<_>>()
      );
      assert_eq!(1, files.len());
      assert!(files[0].1.is_some());
    }
  }
}
