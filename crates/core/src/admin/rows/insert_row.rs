use axum::Json;
use axum::extract::{Path, State};
use serde::{Deserialize, Serialize};
use trailbase_schema::{QualifiedName, QualifiedNameEscaped};
use trailbase_sqlvalue::SqlValue;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::connection::ConnectionEntry;
use crate::records::params::Params;
use crate::records::write_queries::run_insert_query;

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct InsertRowRequest {
  /// Row data, which is expected to be a map from column name to value.
  pub row: indexmap::IndexMap<String, SqlValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InsertRowResponse {
  pub row_id: i64,
}

pub async fn insert_row_handler(
  State(state): State<AppState>,
  Path(table_name): Path<String>,
  Json(request): Json<InsertRowRequest>,
) -> Result<Json<InsertRowResponse>, Error> {
  let table_name = QualifiedName::parse(&table_name)?;
  let ConnectionEntry {
    connection: conn,
    metadata,
  } = state.connection_manager().get_entry_for_qn(&table_name)?;

  let Some(table_metadata) = metadata.get_table(&table_name) else {
    return Err(Error::Precondition(format!(
      "Table {table_name:?} not found"
    )));
  };

  let rowid_value = run_insert_query(
    &conn,
    state.objectstore(),
    &QualifiedNameEscaped::new(&table_metadata.schema.name),
    None,
    "_rowid_",
    // NOTE: We "fancy" parse JSON string values, since the UI currently ships everything as a
    // string. We could consider pushing some more type-awareness into the ui.
    Params::for_admin_insert(table_metadata, request.row)?,
  )
  .await?;

  return match rowid_value {
    trailbase_sqlite::Value::Integer(rowid) => Ok(Json(InsertRowResponse { row_id: rowid })),
    _ => Err(Error::Internal(
      format!("unexpected return type: {rowid_value:?}").into(),
    )),
  };
}

#[cfg(test)]
mod tests {
  use axum::Json;
  use axum::extract::{Path, State};
  use trailbase_sqlvalue::Blob;

  use super::*;
  use crate::app_state::*;

  #[cfg(any(feature = "geos", feature = "geos-static"))]
  #[tokio::test]
  async fn admin_insert_geometry_test() {
    let state = test_state(None).await.unwrap();
    let conn = state.conn();

    conn
      .execute_batch(
        "
         CREATE TABLE IF NOT EXISTS geom_table (
             id       INTEGER PRIMARY KEY,
             geom     BLOB CHECK(ST_IsValid(geom))
         ) STRICT;
        ",
      )
      .await
      .unwrap();

    state.rebuild_connection_metadata().await.unwrap();

    let wkb_geometry: Vec<u8> = {
      use geos::Geom;

      let coords = geos::CoordSeq::new_from_vec(&[&[12.4924, 41.8902]]).unwrap();
      let geometry = geos::Geometry::create_point(coords).unwrap();
      geometry.to_wkb().unwrap()
    };

    let request = InsertRowRequest {
      row: indexmap::IndexMap::from([
        ("id".to_string(), SqlValue::Integer(3)),
        (
          "geom".to_string(),
          SqlValue::Blob(Blob::Array(wkb_geometry)),
        ),
      ]),
    };

    let Json(response) = insert_row_handler(
      State(state.clone()),
      Path("geom_table".into()),
      Json(request),
    )
    .await
    .unwrap();

    assert_eq!(3, response.row_id);
  }
}
