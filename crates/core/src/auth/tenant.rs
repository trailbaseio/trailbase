use const_format::formatcp;
use std::path::PathBuf;
use std::sync::Arc;
use trailbase_extension::jsonschema::JsonSchemaRegistry;
use trailbase_sqlite::{Connection, Row};
use uuid::Uuid;

use crate::auth::AuthError;
use crate::constants::TENANT_TABLE;
use crate::migrations::{get_base_and_custom_migrations, new_migration_runner};
use crate::{AppState, DataDir};

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

pub async fn apply_migrations_for_all_multi_tenant_dbs(
  data_dir: &DataDir,
  json_schema_registry: Arc<JsonSchemaRegistry>,
  config: &crate::config::proto::Config,
  conn: &Connection,
) -> Result<(), crate::InitError> {
  use crate::InitError;

  for DbTenant { name, template, .. } in list_tenants(conn).await? {
    if config
      .tenant_templates
      .iter()
      .find(|t| t.name() == template)
      .is_none()
    {
      return Err(InitError::CustomInit(format!(
        "tenant '{name}' found in DB references template '{template}' not found in config ({:?})",
        config.tenant_templates
      )));
    }

    // Apply migrations.
    let tenant_db_dir = data_dir.tenant_db_dir(&template);
    let migrations_dir = data_dir.template_migrations_dir(&template);
    let migrations = get_base_and_custom_migrations(Some(&migrations_dir))
      .map_err(|err| InitError::CustomInit(format!("{err}")))?;

    let mut conns = find_tenant_dbs(&tenant_db_dir)
      .into_iter()
      .map(|path| {
        return trailbase_extension::connect_rusqlite(
          Some(&path),
          Some(json_schema_registry.clone()),
        )
        .map_err(|err| InitError::CustomInit(format!("{err}")));
      })
      .collect::<Result<Vec<_>, _>>()?;

    // First run a dry-run migration for each tenant (including golden).
    for conn in &mut conns {
      let runner = new_migration_runner(&migrations).set_dry_run(true);
      runner
        .run(conn)
        .map_err(|err| InitError::CustomInit(format!("Dry-run failed: {err}")))?;
    }

    // If nothing failed above, migrate all the DBs.
    for conn in &mut conns {
      let runner = new_migration_runner(&migrations).set_dry_run(false);
      runner
        .run(conn)
        .map_err(|err| InitError::CustomInit(format!("Wet-run failed: {err}")))?;
    }
  }

  return Ok(());
}

async fn list_tenants(conn: &Connection) -> Result<Vec<DbTenant>, AuthError> {
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

fn find_tenant_dbs(path: impl AsRef<std::path::Path>) -> Vec<PathBuf> {
  let Ok(dir) = std::fs::read_dir(path.as_ref()) else {
    return vec![];
  };

  return dir
    .into_iter()
    .flat_map(|entry| {
      let Ok(entry) = entry else {
        return None;
      };

      let Ok(metadata) = entry.metadata() else {
        return None;
      };

      if metadata.is_file() || metadata.is_symlink() {
        let path = entry.path();
        if path.extension()? == "db" {
          return Some(path);
        }
      }

      return None;
    })
    .collect();
}
