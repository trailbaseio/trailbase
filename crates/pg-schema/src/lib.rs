#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use serde::Deserialize;
use trailbase_schema::sqlite::{Column, ColumnOption, QualifiedName, Table};
use trailbase_sqlite::{Connection, params};

#[derive(thiserror::Error, Debug)]
pub enum Error {
  #[error("DB: {0}")]
  Db(#[from] trailbase_sqlite::Error),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct TableInformationSchema {
  table_catalog: String,
  table_schema: String,
  table_name: String,
  table_type: Option<String>,
  is_typed: String,
}

const QUERY_TABLES_WITH_TABLE_CONSTRAINTS: &str = "
SELECT
    t.table_catalog,
    t.table_schema,
    t.table_name,
    t.table_type,
    t.is_typed,
    COUNT(tc.constraint_name) AS total_constraints,
    STRING_AGG(DISTINCT tc.constraint_type, ', ' ORDER BY tc.constraint_type) AS constraint_types,
    STRING_AGG(tc.constraint_name, ', ' ORDER BY tc.constraint_name) AS constraint_names
FROM information_schema.tables t
LEFT JOIN information_schema.table_constraints tc
    ON t.table_schema = tc.table_schema
    AND t.table_name = tc.table_name
WHERE
    t.table_schema NOT IN ('pg_catalog', 'information_schema')
    AND t.table_type = 'BASE TABLE'
GROUP BY
    t.table_catalog,
    t.table_schema,
    t.table_name,
    t.table_type,
    t.is_typed
ORDER BY
    t.table_schema,
    t.table_name;
";

#[allow(unused)]
async fn get_tables(conn: &Connection) -> Result<Vec<TableInformationSchema>, Error> {
  return Ok(
    conn
      .read_query_values(QUERY_TABLES_WITH_TABLE_CONSTRAINTS, ())
      .await?,
  );
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ColumnInformationSchema {
  table_catalog: String,
  table_schema: String,
  table_name: String,
  column_name: String,
  ordinal_position: i32,
  // Is "NO" or "YES" :/.
  data_type: String,
  is_nullable: String,
  column_default: Option<String>,
  // E.g. "NEVER".
  is_generated: String,
  primary_key: Option<String>,
  foreign_key: Option<String>,
  unique_constraint: Option<String>,
  check_constraint: Option<String>,
}

const QUERY_COLUMNS_WITH_CONSTRAINTS: &str = "
SELECT
    c.table_catalog,
    c.table_schema,
    c.table_name,
    c.column_name,
    c.ordinal_position,
    c.data_type,
    c.is_nullable,
    c.column_default,
    c.is_generated,
    MAX(CASE WHEN tc.constraint_type = 'PRIMARY KEY' THEN tc.constraint_name END) AS primary_key,
    MAX(CASE WHEN tc.constraint_type = 'FOREIGN KEY' THEN tc.constraint_name END) AS foreign_key,
    MAX(CASE WHEN tc.constraint_type = 'UNIQUE' THEN tc.constraint_name END) AS unique_constraint,
    MAX(CASE WHEN tc.constraint_type = 'CHECK' THEN tc.constraint_name END) AS check_constraint
FROM information_schema.columns c
LEFT JOIN information_schema.key_column_usage kcu
    ON c.table_schema = kcu.table_schema
    AND c.table_name = kcu.table_name
    AND c.column_name = kcu.column_name
LEFT JOIN information_schema.table_constraints tc
    ON kcu.constraint_name = tc.constraint_name
    AND kcu.table_schema = tc.table_schema
    AND kcu.table_name = tc.table_name
WHERE
    c.table_schema NOT IN ('pg_catalog', 'information_schema')
    AND c.table_name = $1
GROUP BY
    c.table_catalog,
    c.table_schema,
    c.table_name,
    c.column_name,
    c.ordinal_position,
    c.data_type,
    c.is_nullable,
    c.column_default,
    c.is_generated
ORDER BY
    -- c.table_schema,
    -- c.table_name,
    c.ordinal_position;
";

#[allow(unused)]
async fn get_columns(
  conn: &Connection,
  table_name: &str,
) -> Result<Vec<ColumnInformationSchema>, Error> {
  return Ok(
    conn
      .read_query_values(
        QUERY_COLUMNS_WITH_CONSTRAINTS,
        params!(table_name.to_string()),
      )
      .await?,
  );
}

#[allow(unused)]
async fn build_table_schema(
  conn: &Connection,
  table: TableInformationSchema,
) -> Result<Table, Error> {
  let TableInformationSchema {
    table_catalog: _,
    table_schema,
    table_name,
    table_type: _,
    is_typed,
  } = table;

  let columns = get_columns(conn, &table_name).await?;

  // let foreign_keys: Vec<_> = columns
  //   .iter()
  //   .filter_map(|c| {
  //     if let Some(ref fk) = c.foreign_key {
  //       return Some(ForeignKey {
  //         name: None,
  //         columns: vec![],
  //         foreign_table: fk.clone(),
  //         referred_columns: vec![],
  //         on_delete: None,
  //         on_update: None,
  //       });
  //     }
  //     return None;
  //   })
  //   .collect();
  //
  // let unique: Vec<_> = columns
  //   .iter()
  //   .filter_map(|c| {
  //     if let Some(ref unique) = c.unique_constraint {
  //       return Some(UniqueConstraint {
  //         name: None,
  //         columns: vec![unique.clone()],
  //         conflict_clause: None,
  //       });
  //     }
  //     return None;
  //   })
  //   .collect();
  //
  // let checks: Vec<_> = columns
  //   .iter()
  //   .filter_map(|c| {
  //     if let Some(ref check) = c.check_constraint {
  //       return Some(Check {
  //         name: None,
  //         expr: check.clone(),
  //       });
  //     }
  //     return None;
  //   })
  //   .collect();

  return Ok(Table {
    name: QualifiedName {
      name: table_name,
      database_schema: Some(table_schema),
    },
    strict: is_typed == "NO",
    columns: columns
      .into_iter()
      .map(|c| {
        let mut options = Vec::<ColumnOption>::new();

        if c.is_nullable == "NO" {
          options.push(ColumnOption::NotNull)
        }

        if let Some(default) = c.column_default {
          options.push(ColumnOption::Default(default));
        }

        if c.is_generated != "NEVER" {
          options.push(ColumnOption::Generated {
            expr: "TODO".to_string(),
            mode: None,
          })
        }

        if let Some(unique) = c.unique_constraint {
          options.push(ColumnOption::Unique {
            is_primary: unique.contains("pkey"),
            conflict_clause: None,
          });
        }

        if let Some(fk) = c.foreign_key {
          options.push(ColumnOption::ForeignKey {
            foreign_table: fk,
            referred_columns: vec![],
            on_delete: None,
            on_update: None,
          })
        }

        if let Some(check) = c.check_constraint {
          options.push(ColumnOption::Check(check));
        }

        return Column {
          name: c.column_name,
          type_name: c.data_type,
          data_type: trailbase_schema::sqlite::ColumnDataType::Any,
          affinity_type: trailbase_schema::sqlite::ColumnAffinityType::Integer,
          options,
        };
      })
      .collect(),
    // QUESTION: These are table level constraints.
    foreign_keys: vec![],
    unique: vec![],
    checks: vec![],
    virtual_table: false,
    temporary: false,
  });
}

#[cfg(test)]
mod tests {
  use trailbase_sqlite::Connection;

  use super::*;

  pub async fn test_connection() -> (pglite_oxide::PgliteServer, Connection) {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // NOTE: `db.connection_uri()` returns rubbish for UDS.
    let sock = temp_dir.path().join(".s.PGSQL.5432");

    let db = pglite_oxide::PgliteServer::builder()
      .fresh_temporary()
      // .temporary()
      .unix(&sock)
      .start()
      .unwrap();

    let pg_uri = format!(
      "postgresql://postgres@/template1?host={}",
      temp_dir.path().to_string_lossy()
    );

    return (
      db,
      trailbase_sqlite::Connection::pg_with_opts(trailbase_sqlite::generic::PgOptions {
        connection: trailbase_sqlite::generic::PgConnection::Uri(pg_uri),
        num_threads: Some(1),
      })
      .unwrap(),
    );
  }

  #[tokio::test]
  async fn postgres_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE TABLE table0 (id INTEGER PRIMARY KEY NOT NULL);

        CREATE TABLE table1 (
          id     INTEGER PRIMARY KEY,
          fk     INTEGER REFERENCES table0(id),
          a      TEXT DEFAULT ('foo'),
          b      INT8 NOT NULL DEFAULT (5)
        );
      ",
      )
      .await
      .unwrap();

    let tables = get_tables(&conn).await.unwrap();

    assert!(tables.iter().any(|t| t.table_name == "table0"));
    assert!(tables.iter().any(|t| t.table_name == "table1"));

    let columns0 = get_columns(&conn, "table0").await.unwrap();
    assert!(columns0.len() > 0);

    assert_eq!(
      columns0,
      vec![ColumnInformationSchema {
        table_catalog: "template1".to_string(),
        table_schema: "public".to_string(),
        table_name: "table0".to_string(),
        column_name: "id".to_string(),
        ordinal_position: 1,
        is_nullable: "NO".to_string(),
        data_type: "integer".to_string(),
        column_default: None,
        is_generated: "NEVER".to_string(),
        primary_key: Some("table0_pkey".to_string()),
        foreign_key: None,
        unique_constraint: None,
        check_constraint: None,
      },]
    );

    let columns1 = get_columns(&conn, "table1").await.unwrap();
    assert!(columns1.len() > 0);

    assert_eq!(
      columns1,
      vec![
        ColumnInformationSchema {
          table_catalog: "template1".to_string(),
          table_schema: "public".to_string(),
          table_name: "table1".to_string(),
          column_name: "id".to_string(),
          ordinal_position: 1,
          is_nullable: "NO".to_string(),
          data_type: "integer".to_string(),
          column_default: None,
          is_generated: "NEVER".to_string(),
          primary_key: Some("table1_pkey".to_string()),
          foreign_key: None,
          unique_constraint: None,
          check_constraint: None,
        },
        ColumnInformationSchema {
          table_catalog: "template1".to_string(),
          table_schema: "public".to_string(),
          table_name: "table1".to_string(),
          column_name: "fk".to_string(),
          ordinal_position: 2,
          is_nullable: "YES".to_string(),
          data_type: "integer".to_string(),
          column_default: None,
          is_generated: "NEVER".to_string(),
          primary_key: None,
          foreign_key: Some("table1_fk_fkey".to_string()),
          unique_constraint: None,
          check_constraint: None,
        },
        ColumnInformationSchema {
          table_catalog: "template1".to_string(),
          table_schema: "public".to_string(),
          table_name: "table1".to_string(),
          column_name: "a".to_string(),
          ordinal_position: 3,
          is_nullable: "YES".to_string(),
          data_type: "text".to_string(),
          column_default: Some("'foo'::text".to_string()),
          is_generated: "NEVER".to_string(),
          primary_key: None,
          foreign_key: None,
          unique_constraint: None,
          check_constraint: None,
        },
        ColumnInformationSchema {
          table_catalog: "template1".to_string(),
          table_schema: "public".to_string(),
          table_name: "table1".to_string(),
          column_name: "b".to_string(),
          ordinal_position: 4,
          is_nullable: "NO".to_string(),
          data_type: "bigint".to_string(),
          column_default: Some("5".to_string()),
          is_generated: "NEVER".to_string(),
          primary_key: None,
          foreign_key: None,
          unique_constraint: None,
          check_constraint: None,
        },
      ]
    );

    println!("COLUMNS1: {columns1:?}\n");
  }
}
