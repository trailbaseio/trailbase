use axum::{
  Json,
  extract::{RawQuery, State},
};
use lazy_static::lazy_static;
use serde::Serialize;
use std::borrow::Cow;
use trailbase_qs::{Cursor, Order, OrderPrecedent, Query};
use trailbase_schema::QualifiedName;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::user::DbUser;
use crate::connection::ConnectionEntry;
use crate::constants::USER_TABLE;
use crate::listing::{WhereClause, build_filter_where_clause, limit_or_default};
use crate::util::id_to_b64;

#[derive(Debug, Serialize, TS)]
pub struct UserJson {
  pub id: String,
  pub email: String,
  pub verified: bool,
  pub admin: bool,

  // For external oauth providers.
  pub provider_id: i64,
  pub provider_user_id: Option<String>,

  pub email_verification_code: String,
}

impl From<DbUser> for UserJson {
  fn from(value: DbUser) -> Self {
    UserJson {
      id: Uuid::from_bytes(value.id).to_string(),
      email: value.email,
      verified: value.verified,
      admin: value.admin,
      provider_id: value.provider_id,
      provider_user_id: value.provider_user_id,
      email_verification_code: value.email_verification_code.unwrap_or_default(),
    }
  }
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ListUsersResponse {
  total_row_count: i64,
  cursor: Option<String>,

  users: Vec<UserJson>,
}

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
    cursor,
    order,
    filter: filter_params,
    ..
  } = raw_url_query
    .as_ref()
    .map_or_else(|| Ok(Query::default()), |query| Query::parse(query))
    .map_err(|err| {
      return Error::BadRequest(format!("Invalid query '{err}': {raw_url_query:?}").into());
    })?;

  let Some(table_metadata) = metadata.get_table(&QualifiedName::parse(USER_TABLE)?) else {
    return Err(Error::Precondition(format!("Table {USER_TABLE} not found")));
  };
  // Where clause contains column filters and cursor depending on what's present in the url query
  // string.
  let filter_where_clause =
    build_filter_where_clause("_ROW_", &table_metadata.schema.columns, filter_params)?;

  let total_row_count: i64 = conn
    .read_query_row_f(
      format!(
        "SELECT COUNT(*) FROM {USER_TABLE} AS _ROW_ WHERE {where_clause}",
        where_clause = filter_where_clause.clause
      ),
      filter_where_clause.params.clone(),
      |row| row.get(0),
    )
    .await?
    .unwrap_or(-1);

  lazy_static! {
    static ref DEFAULT_ORDERING: Order = Order {
      columns: vec![("_rowid_".to_string(), OrderPrecedent::Descending)],
    };
  }
  let users = fetch_users(
    &conn,
    filter_where_clause.clone(),
    if let Some(cursor) = cursor {
      Some(
        Cursor::parse(&cursor, trailbase_qs::CursorType::Blob)
          .map_err(|err| Error::BadRequest(err.into()))?,
      )
    } else {
      None
    },
    order.as_ref().unwrap_or_else(|| &DEFAULT_ORDERING),
    limit_or_default(limit, None).map_err(|err| Error::BadRequest(err.into()))?,
  )
  .await?;

  return Ok(Json(ListUsersResponse {
    total_row_count,
    cursor: users.last().map(|user| id_to_b64(&user.id)),
    users: users
      .into_iter()
      .map(|user| user.into())
      .collect::<Vec<UserJson>>(),
  }));
}

async fn fetch_users(
  conn: &trailbase_sqlite::Connection,
  filter_where_clause: WhereClause,
  cursor: Option<Cursor>,
  order: &Order,
  limit: usize,
) -> Result<Vec<DbUser>, Error> {
  let mut params = filter_where_clause.params;
  let mut where_clause = filter_where_clause.clause;

  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(limit as i64),
  ));

  if let Some(cursor) = cursor {
    params.push((
      Cow::Borrowed(":cursor"),
      crate::admin::util::cursor_to_value(cursor),
    ));
    where_clause = format!("{where_clause} AND _ROW_.id < :cursor",);
  }

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
      SELECT _ROW_.*
      FROM {USER_TABLE} as _ROW_
      WHERE
        {where_clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
    "#,
  );

  let users = conn.read_query_values::<DbUser>(sql_query, params).await?;
  return Ok(users);
}
