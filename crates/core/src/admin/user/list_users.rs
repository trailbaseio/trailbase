use axum::{
  Json,
  extract::{RawQuery, State},
};
use serde::Serialize;
use std::borrow::Cow;
use trailbase_qs::{Order, OrderPrecedent, Query};
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::user::DbUser;
use crate::connection::ConnectionEntry;
use crate::constants::{TENANT_MEMBER_TABLE, USER_TABLE_FQ};
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::util::row_id_column;

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
pub struct UserJson {
  pub id: String,
  pub email: Option<String>,
  pub unverified_email: Option<String>,
  pub username: Option<String>,
  pub admin: bool,

  // For external oauth providers.
  pub provider_id: i64,
  pub provider_user_id: Option<String>,

  pub created: i64,
  pub updated: i64,

  pub tenant_memberships: Vec<String>,
}

impl UserJson {
  fn from(user: DbUser, tenant_memberships: Vec<String>) -> Self {
    UserJson {
      id: Uuid::from_bytes(user.id).to_string(),
      email: user.email.map(String::from),
      unverified_email: user.unverified_email.map(String::from),
      username: user.username.map(String::from),
      admin: user.admin,
      provider_id: user.provider_id,
      provider_user_id: user.provider_user_id.map(String::from),
      created: user.created,
      updated: user.updated,
      tenant_memberships,
    }
  }
}

#[derive(Debug, Serialize, TS, utoipa::ToSchema)]
#[ts(export)]
pub struct ListUsersResponse {
  total_row_count: i64,

  users: Vec<UserJson>,
}

#[utoipa::path(
  get,
  path = "/user",
  tag = "admin",
  responses(
    (status = 200, description = "Success"),
  )
)]
pub async fn list_users_handler(
  State(state): State<AppState>,
  RawQuery(raw_url_query): RawQuery,
) -> Result<Json<ListUsersResponse>, Error> {
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

  let Some(table_metadata) = metadata.get_table(&USER_TABLE_FQ) else {
    // QUESTION: Should this actually be an internal error? Hardly a user failure.
    log::debug!("User table not found: {metadata:?}");
    return Err(Error::Precondition(format!(
      "Table {} not found",
      USER_TABLE_FQ.escaped_string()
    )));
  };
  // Where clause contains column filters and offset depending on what's present in the url query
  // string.
  let filter_where_clause =
    build_filter_where_clause("_ROW_", &table_metadata.column_metadata, filter_params)?;

  let total_row_count: i64 = conn
    .read_query_row_get(
      format!(
        "SELECT COUNT(*) FROM {fq_table_name} AS _ROW_ WHERE {where_clause}",
        fq_table_name = USER_TABLE_FQ.escaped_string(),
        where_clause = filter_where_clause.clause
      ),
      filter_where_clause.params.clone(),
      0,
    )
    .await?
    .unwrap_or(-1);

  let users = fetch_users(
    &conn,
    filter_where_clause.clone(),
    offset,
    &order.unwrap_or_else(|| Order {
      columns: vec![(
        row_id_column(conn.connection_type()).to_string(),
        OrderPrecedent::Descending,
      )],
    }),
    limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))?,
  )
  .await?;

  return Ok(Json(ListUsersResponse {
    total_row_count,
    users: users
      .into_iter()
      .map(|(user, tenant_memberships)| UserJson::from(user, tenant_memberships))
      .collect(),
  }));
}

async fn fetch_users(
  conn: &trailbase_sqlite::Connection,
  filter_where_clause: WhereClause,
  offset: Option<usize>,
  order: &Order,
  limit: usize,
) -> Result<Vec<(DbUser, Vec<String>)>, Error> {
  let WhereClause {
    mut params,
    clause: where_clause,
  } = filter_where_clause;

  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(limit as i64),
  ));
  params.push((
    Cow::Borrowed(":offset"),
    trailbase_sqlite::Value::Integer(offset.unwrap_or(0) as i64),
  ));

  let order_clause = order
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

  let sql_query = format!(
    r#"
      SELECT _ROW_.*, json_group_array(uuid_text(m.tenant))
        FROM {fq_table_name} AS _ROW_
        LEFT JOIN {TENANT_MEMBER_TABLE} AS m ON m.user = _ROW_.id
      WHERE
        {where_clause}
      GROUP BY _ROW_.id
      ORDER BY
        {order_clause}
      LIMIT :limit
      OFFSET :offset;
    "#,
    fq_table_name = USER_TABLE_FQ.escaped_string(),
  );

  let users: Vec<(DbUser, Vec<String>)> = conn
    .read_query_rows(sql_query.clone(), params)
    .await
    .map_err(|err| {
      log::error!("fetch users failed '{sql_query}': {err:?}");
      return err;
    })?
    .into_iter()
    .map(|row| -> Result<_, Error> {
      let memberships: Option<String> = row.get(row.len() - 1)?;
      return Ok((
        DbUser::from_row(row)?,
        memberships
          .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
          .unwrap_or_default(),
      ));
    })
    .collect::<Result<_, _>>()?;

  return Ok(users);
}
