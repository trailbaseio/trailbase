use axum::{
  extract::{RawQuery, State},
  Json,
};
use lazy_static::lazy_static;
use serde::Serialize;
use std::borrow::Cow;
use ts_rs::TS;
use uuid::Uuid;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;
use crate::auth::user::DbUser;
use crate::constants::{USER_TABLE, USER_TABLE_ID_COLUMN};
use crate::listing::{
  build_filter_where_clause, limit_or_default, parse_and_sanitize_query, Cursor, Order,
  QueryParseResult, WhereClause,
};
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
  let conn = state.user_conn();

  // TODO: we should probably return an error if the query parsing fails rather than quietly
  // falling back to defaults.
  let QueryParseResult {
    params: filter_params,
    cursor,
    limit,
    order,
    ..
  } = parse_and_sanitize_query(raw_url_query.as_deref())
    .map_err(|err| Error::Precondition(format!("Invalid query '{err}': {raw_url_query:?}")))?;

  let Some(table_metadata) = state.table_metadata().get(USER_TABLE) else {
    return Err(Error::Precondition(format!("Table {USER_TABLE} not found")));
  };
  // Where clause contains column filters and cursor depending on what's present in the url query
  // string.
  let filter_where_clause =
    build_filter_where_clause(&table_metadata.schema.columns, filter_params)?;

  let total_row_count: i64 = conn
    .query_value(
      &format!(
        "SELECT COUNT(*) FROM {USER_TABLE} WHERE {where_clause}",
        where_clause = filter_where_clause.clause
      ),
      filter_where_clause.params.clone(),
    )
    .await?
    .unwrap_or(-1);

  lazy_static! {
    static ref DEFAULT_ORDERING: Vec<(String, Order)> =
      vec![(USER_TABLE_ID_COLUMN.to_string(), Order::Descending)];
  }
  let users = fetch_users(
    conn,
    filter_where_clause.clone(),
    cursor,
    order.unwrap_or_else(|| DEFAULT_ORDERING.clone()),
    limit_or_default(limit),
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
  order: Vec<(String, Order)>,
  limit: usize,
) -> Result<Vec<DbUser>, Error> {
  let mut params = filter_where_clause.params;
  let mut where_clause = filter_where_clause.clause;

  params.push((
    Cow::Borrowed(":limit"),
    trailbase_sqlite::Value::Integer(limit as i64),
  ));

  if let Some(cursor) = cursor {
    params.push((Cow::Borrowed(":cursor"), cursor.into()));
    where_clause = format!("{where_clause} AND _ROW_.id < :cursor",);
  }

  let order_clause = order
    .iter()
    .map(|(col, ord)| {
      format!(
        r#"_ROW_."{col}" {}"#,
        match ord {
          Order::Descending => "DESC",
          Order::Ascending => "ASC",
        }
      )
    })
    .collect::<Vec<_>>()
    .join(", ");

  let sql_query = format!(
    r#"
      SELECT _ROW_.*
      FROM
        (SELECT * FROM {USER_TABLE}) as _ROW_
      WHERE
        {where_clause}
      ORDER BY
        {order_clause}
      LIMIT :limit
    "#,
  );

  let users = conn.query_values::<DbUser>(&sql_query, params).await?;
  return Ok(users);
}
