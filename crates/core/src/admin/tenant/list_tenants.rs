use axum::{
  Json,
  extract::{RawQuery, State},
};
use serde::Serialize;
use std::borrow::Cow;
use trailbase_qs::{Order, OrderPrecedent, Query};
use trailbase_schema::QualifiedName;
use ts_rs::TS;
use uuid::Uuid;

use crate::app_state::AppState;
use crate::auth::tenant::DbTenant;
use crate::connection::ConnectionEntry;
use crate::constants::TENANT_TABLE;
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::util::row_id_column;
use crate::{admin::AdminError as Error, auth::AuthError};

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
pub struct TenantJson {
  pub id: Uuid,
  pub name: String,
  pub template: String,

  pub created: i64,
}

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ListTenantsResponse {
  pub total_row_count: i64,
  pub tenants: Vec<TenantJson>,
}

#[utoipa::path(
  get,
  path = "/tenant",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn list_tenants_handler(
  State(state): State<AppState>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListTenantsResponse>, Error> {
  let ConnectionEntry {
    connection: conn,
    metadata,
  } = state.connection_manager().main_entry();

  let Query {
    limit,
    offset,
    order,
    filter: filter_params,
    ..
  } = raw_url_query
    .as_ref()
    .map_or_else(|| Ok(Query::default()), |query| Query::parse(query))
    .map_err(|err| {
      return Error::BadRequest(format!("Invalid query '{err}': {raw_url_query:?}").into());
    })?;

  let Some(table_metadata) = metadata.get_table(&QualifiedName {
    name: TENANT_TABLE.to_string(),
    database_schema: None,
  }) else {
    // QUESTION: Should this actually be an internal error? Hardly a user failure.
    log::debug!("User table not found: {metadata:?}");
    return Err(Error::Precondition(format!(
      "Table {TENANT_TABLE} not found"
    )));
  };
  // Where clause contains column filters and offset depending on what's present in the url query
  // string.
  let filter_where_clause =
    build_filter_where_clause("_ROW_", &table_metadata.column_metadata, filter_params)?;

  let WhereClause {
    mut params,
    clause: ref where_clause,
  } = filter_where_clause;

  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(
      limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))? as i64,
    ),
  ));
  params.push((
    Cow::Borrowed(":offset"),
    trailbase_sqlite::Value::Integer(offset.unwrap_or(0) as i64),
  ));

  let order_clause = order
    .unwrap_or_else(|| Order {
      columns: vec![(
        row_id_column(conn.connection_type()).to_string(),
        OrderPrecedent::Descending,
      )],
    })
    .columns
    .iter()
    .map(|(col, ord)| {
      format!(
        r#"_ROW_."{col}" {}"#,
        match ord {
          OrderPrecedent::Descending => "DESC",
          OrderPrecedent::Ascending => "ASC",
        }
      )
    })
    .collect::<Vec<_>>()
    .join(", ");

  let total_row_count: i64 = conn
    .read_query_row_get(
      format!(
        "SELECT COUNT(*) FROM {TENANT_TABLE} AS _ROW_ WHERE {where_clause}",
        where_clause = filter_where_clause.clause
      ),
      params.clone(),
      0,
    )
    .await?
    .unwrap_or(-1);

  let tenants = conn
    .read_query_rows(
      format!(
        "
          SELECT _ROW_.* FROM {TENANT_TABLE} AS _ROW_
          WHERE {where_clause}
          ORDER BY
            {order_clause}
          LIMIT :limit
          OFFSET :offset;
        ",
      ),
      params.clone(),
    )
    .await?
    .into_iter()
    .map(|row| -> Result<_, AuthError> {
      let DbTenant {
        id,
        name,
        template,
        created,
      } = DbTenant::from_row(row)?;

      return Ok(TenantJson {
        id: Uuid::from_bytes(id),
        name,
        template,
        created,
      });
    })
    .collect::<Result<Vec<_>, _>>()?;

  return Ok(Json(ListTenantsResponse {
    total_row_count,
    tenants,
  }));
}
