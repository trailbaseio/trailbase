use const_format::formatcp;
use trailbase_sqlite::{Connection, Row};
use uuid::Uuid;

use crate::AppState;
use crate::auth::AuthError;
use crate::constants::TENANT_TABLE;

#[derive(Debug, Default, Clone)]
pub struct DbTenant {
  #[allow(unused)]
  pub id: [u8; 16],
  pub name: String,
  pub template: String,
  #[allow(unused)]
  pub created: i64,
}

impl DbTenant {
  pub const COLUMNS: &str = "id, name, template, created";

  pub fn uuid(&self) -> Uuid {
    return Uuid::from_bytes(self.id);
  }

  pub fn from_row(row: Row) -> Result<Self, AuthError> {
    #[inline]
    fn from_row_impl(mut row: Row) -> Result<DbTenant, trailbase_sqlite::from_sql::FromSqlError> {
      // Sanity check.
      #[cfg(debug_assertions)]
      for (i, column) in DbTenant::COLUMNS.split(",").enumerate() {
        assert_eq!(Some(column.trim()), row.column(i).map(|c| c.name.as_str()));
      }

      return Ok(DbTenant {
        id: row.get(0)?,
        name: row.consume_value(1)?.try_into()?,
        template: row.consume_value(2)?.try_into()?,
        created: row.get(3)?,
      });
    }

    // Should never fail. This means there's a schema mismatch.
    return from_row_impl(row).map_err(|err| AuthError::Internal(err.into()));
  }
}

pub async fn list_tenants(conn: &Connection) -> Result<Vec<DbTenant>, AuthError> {
  let rows = conn
    .read_query_rows(formatcp!("SELECT * FROM \"{TENANT_TABLE}\""), ())
    .await?;
  return rows
    .into_iter()
    .map(DbTenant::from_row)
    .collect::<Result<Vec<_>, _>>();
}

pub async fn create_new_tenant(
  state: &AppState,
  template: String,
  name: String,
) -> Result<Uuid, AuthError> {
  let templates = state.access_config(|c| c.tenant_templates.clone());

  if templates
    .iter()
    .find(|t| t.name.as_ref() == Some(&template))
    .is_none()
  {
    return Err(AuthError::BadRequest("template not found"));
  }

  const INSERT_TENANT_QUERY: &str = formatcp!(
    "\
      INSERT INTO \"{TENANT_TABLE}\" \
        (template, name) \
      VALUES \
        (:template, :name) \
      RETURNING {columns} \
    ",
    columns = DbTenant::COLUMNS,
  );

  let Some(tenant) = state
    .user_conn()
    .write_query_row(
      INSERT_TENANT_QUERY,
      trailbase_sqlite::named_params! {
        ":name": name,
        ":template": template,
      },
    )
    .await?
    .map(DbTenant::from_row)
    .transpose()?
  else {
    return Err(AuthError::Internal("Insert returned no tenant".into()));
  };

  return Ok(Uuid::from_bytes(tenant.id));
}
