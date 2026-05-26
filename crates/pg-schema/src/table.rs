use serde::Deserialize;
use trailbase_schema::sqlite::{
  Column, ColumnAffinityType, ColumnDataType, ColumnOption, QualifiedName, Table,
};
use trailbase_sqlite::params;

use crate::error::Error;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TableInformationSchema {
  pub table_catalog: String,
  pub table_schema: String,
  pub table_name: String,
  pub table_type: Option<String>,
  pub is_typed: String,
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
LEFT JOIN information_schema.table_constraints tc ON
    t.table_schema = tc.table_schema
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

fn get_tables(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<TableInformationSchema>, Error> {
  return conn
    .query_rows(QUERY_TABLES_WITH_TABLE_CONSTRAINTS, ())?
    .into_iter()
    .map(|row| {
      return Ok(TableInformationSchema {
        table_catalog: row.get(0)?,
        table_schema: row.get(1)?,
        table_name: row.get(2)?,
        table_type: row.get(3)?,
        is_typed: row.get(4)?,
      });
    })
    .collect::<Result<_, Error>>();
}

#[derive(Clone, Default, Debug, Deserialize, PartialEq)]
pub(crate) struct ColumnInformationSchema {
  pub table_catalog: String,
  pub table_schema: String,
  pub table_name: String,
  pub column_name: String,
  pub ordinal_position: i32,
  // Is "NO" or "YES" :/.
  pub data_type: String,
  pub is_nullable: String,
  pub column_default: Option<String>,
  // E.g. "NEVER".
  pub is_generated: String,
  pub primary_key: Option<String>,
  pub foreign_key: Option<String>,
  pub unique_constraint: Option<String>,
  pub check_constraint: Option<String>,
  // For FKs:
  pub fk_table_name: Option<String>,
  pub fk_column_name: Option<String>,
  // For VIEWs:
  pub source_table: Option<String>,
  pub source_column: Option<String>,
  // For Check:
  pub check_clause: Option<String>,
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
    MAX(CASE WHEN tc.constraint_type = 'CHECK' THEN tc.constraint_name END) AS check_constraint,
    -- Only for FKs
    MAX(CASE WHEN tc.constraint_type = 'FOREIGN KEY' THEN fk_ccu.table_name END) AS fk_table_name,
    MAX(CASE WHEN tc.constraint_type = 'FOREIGN KEY' THEN fk_ccu.column_name END) AS fk_column_name,
    -- Only defined for VIEW columns:
    vcu.table_name AS source_table,
    vcu.column_name AS source_column,
    -- Only Checks
    MAX(chc.check_clause) AS check_clause
FROM
    information_schema.columns c
    LEFT JOIN information_schema.constraint_column_usage ccu ON
        c.table_catalog = ccu.table_catalog
        AND c.table_schema = ccu.table_schema
        AND c.table_name = ccu.table_name
        AND c.column_name = ccu.column_name
    -- We need this atop `ccu` above to look up FKs
    LEFT JOIN information_schema.key_column_usage kcu ON
        c.table_catalog = kcu.table_catalog
        AND c.table_schema = kcu.table_schema
        AND c.table_name = kcu.table_name
        AND c.column_name = kcu.column_name
    LEFT JOIN information_schema.table_constraints tc ON
        tc.constraint_catalog IN (ccu.constraint_catalog, kcu.constraint_catalog)
        AND tc.constraint_schema IN (ccu.constraint_schema, kcu.constraint_schema)
        AND (
          (
            ccu.constraint_name = tc.constraint_name
            -- Avoid back-edges.
            AND ccu.table_name = tc.table_name
          ) OR (
            -- Use `kcu` only for FK lookups.
            kcu.constraint_name = tc.constraint_name AND tc.constraint_type = 'FOREIGN KEY'
          )
        )
    LEFT JOIN information_schema.constraint_column_usage fk_ccu ON
        tc.constraint_type = 'FOREIGN KEY'
        AND tc.constraint_catalog = fk_ccu.constraint_catalog
        AND tc.constraint_schema = fk_ccu.constraint_schema
        AND tc.constraint_name = fk_ccu.constraint_name
    LEFT JOIN information_schema.check_constraints chc ON
        ccu.constraint_catalog = tc.constraint_catalog
        AND ccu.constraint_schema = tc.constraint_schema
        AND ccu.constraint_name = chc.constraint_name
    LEFT JOIN information_schema.view_column_usage vcu ON
        c.table_schema = vcu.view_schema
        AND c.table_name = vcu.view_name
        AND c.column_name = vcu.column_name
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
    c.is_generated,
    vcu.table_name,
    vcu.column_name
ORDER BY
    c.ordinal_position;
";

pub(crate) fn get_columns(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
  table_name: &str,
) -> Result<Vec<ColumnInformationSchema>, Error> {
  return conn
    .query_rows(
      QUERY_COLUMNS_WITH_CONSTRAINTS,
      params!(table_name.to_string()),
    )?
    .into_iter()
    .map(|row| {
      return Ok(ColumnInformationSchema {
        table_catalog: row.get(0)?,
        table_schema: row.get(1)?,
        table_name: row.get(2)?,
        column_name: row.get(3)?,
        ordinal_position: row.get(4)?,
        data_type: row.get(5)?,
        is_nullable: row.get(6)?,
        column_default: row.get(7)?,
        is_generated: row.get(8)?,
        primary_key: row.get(9)?,
        foreign_key: row.get(10)?,
        unique_constraint: row.get(11)?,
        check_constraint: row.get(12)?,
        fk_table_name: row.get(13)?,
        fk_column_name: row.get(14)?,
        source_table: row.get(15)?,
        source_column: row.get(16)?,
        check_clause: row.get(17)?,
      });
    })
    .collect::<Result<_, Error>>();
}

fn infer_data_type(type_name: &str) -> ColumnDataType {
  // NOTE: This is basically `FromSql`, i.e. how we map PG types to `Value`..
  // NOTE: We may want to consider using tid.
  return match type_name {
    "uuid" | "bytea" => ColumnDataType::Blob,
    "real" | "double" => ColumnDataType::Real,
    "text" | "varchar" => ColumnDataType::Text,
    name if name.contains("int") || name.contains("bool") || name.contains("serial") => {
      ColumnDataType::Integer
    }
    _ => ColumnDataType::Any,
  };
}

fn infer_affinity_type(type_name: &str) -> ColumnAffinityType {
  // NOTE: Affinity type is really an weakly-typed SQLite thing, we may want to make this
  // optional. Do we even need it at all beyond parsing?
  return match type_name {
    "uuid" | "bytea" => ColumnAffinityType::Blob,
    "real" | "double" => ColumnAffinityType::Real,
    "text" | "varchar" => ColumnAffinityType::Text,
    name if name.contains("int") || name.contains("bool") || name.contains("serial") => {
      ColumnAffinityType::Integer
    }
    _ => ColumnAffinityType::Blob,
  };
}

pub(crate) fn build_column_schema(c: ColumnInformationSchema) -> Result<Column, Error> {
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

  if c.unique_constraint.is_some() || c.primary_key.is_some() {
    options.push(ColumnOption::Unique {
      is_primary: c.primary_key.is_some(),
      conflict_clause: None,
    });
  }

  if let (Some(_fk), Some(t), Some(c)) = (c.foreign_key, c.fk_table_name, c.fk_column_name) {
    options.push(ColumnOption::ForeignKey {
      foreign_table: t,
      referred_columns: vec![c],
      on_delete: None,
      on_update: None,
    })
  }

  if let Some(clause) = c.check_clause {
    options.push(ColumnOption::Check(clause));
  }

  return Ok(Column {
    name: c.column_name,
    data_type: infer_data_type(&c.data_type),
    affinity_type: infer_affinity_type(&c.data_type),
    type_name: c.data_type,
    options,
  });
}

pub fn build_table_schema(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
  table: TableInformationSchema,
) -> Result<Table, Error> {
  let TableInformationSchema {
    table_catalog: _,
    table_schema,
    table_name,
    table_type: _,
    is_typed,
  } = table;

  let columns = get_columns(conn, &table_name)?;

  return Ok(Table {
    name: QualifiedName {
      name: table_name,
      database_schema: Some(table_schema),
    },
    strict: is_typed == "NO",
    columns: columns
      .into_iter()
      .map(build_column_schema)
      .collect::<Result<Vec<_>, _>>()?,
    // FIXME: Add table-level (as opposed to column) constraints.
    foreign_keys: vec![],
    unique: vec![],
    checks: vec![],
    virtual_table: false,
    temporary: false,
  });
}

pub fn build_all_table_schemas(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<Table>, Error> {
  let tables = get_tables(conn)?;

  log::debug!("Found tables: {tables:?}");

  return tables
    .into_iter()
    .map(|table| build_table_schema(conn, table))
    .collect::<Result<Vec<Table>, Error>>();
}

#[cfg(test)]
mod tests {
  use trailbase_sqlite::Connection;

  use super::*;
  use crate::util::test_connection;

  async fn get_tables_async(conn: &Connection) -> Result<Vec<TableInformationSchema>, Error> {
    return Ok(
      conn
        .read_query_values(QUERY_TABLES_WITH_TABLE_CONSTRAINTS, ())
        .await?,
    );
  }

  async fn get_columns_async(
    conn: &Connection,
    table_name: &str,
  ) -> Result<Vec<ColumnInformationSchema>, Error> {
    return Ok(
      conn
        .call_writer({
          let table_name = table_name.to_string();
          move |mut conn| {
            get_columns(&mut conn, &table_name)
              .map_err(|err| trailbase_sqlite::Error::Other(err.into()))
          }
        })
        .await
        .map_err(|err| {
          return match trailbase_sqlite::unpack_other_error::<Error>(err) {
            Ok(err) => err,
            Err(sql_err) => sql_err.into(),
          };
        })?,
    );
  }

  #[tokio::test]
  async fn postgres_column_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE TABLE table0 (
          id     INTEGER PRIMARY KEY NOT NULL CHECK(id >= 0)
        );

        CREATE TABLE table1 (
          id     INTEGER PRIMARY KEY,
          fk0    INTEGER REFERENCES table0(id),
          fk1    INTEGER NOT NULL REFERENCES table0,
          a      TEXT UNIQUE DEFAULT('foo') CHECK(a != ''),
          b      INT8 NOT NULL DEFAULT(5)
        );
      ",
      )
      .await
      .unwrap();

    let tables = get_tables_async(&conn).await.unwrap();

    assert!(tables.iter().any(|t| t.table_name == "table0"));
    assert!(tables.iter().any(|t| t.table_name == "table1"));

    let columns0 = get_columns_async(&conn, "table0").await.unwrap();
    assert!(columns0.len() > 0);

    assert_eq!(
      columns0[0],
      ColumnInformationSchema {
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
        check_constraint: Some("table0_id_check".to_string()),
        check_clause: Some("(id >= 0)".to_string()),
        ..Default::default()
      }
    );

    let columns1 = get_columns_async(&conn, "table1").await.unwrap();
    assert!(columns1.len() > 0);

    assert_eq!(
      columns1[0],
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
        ..Default::default()
      }
    );

    assert_eq!(
      columns1[1],
      ColumnInformationSchema {
        table_catalog: "template1".to_string(),
        table_schema: "public".to_string(),
        table_name: "table1".to_string(),
        column_name: "fk0".to_string(),
        ordinal_position: 2,
        is_nullable: "YES".to_string(),
        data_type: "integer".to_string(),
        column_default: None,
        is_generated: "NEVER".to_string(),
        primary_key: None,
        foreign_key: Some("table1_fk0_fkey".to_string()),
        unique_constraint: None,
        check_constraint: None,
        fk_table_name: Some("table0".to_string()),
        fk_column_name: Some("id".to_string()),
        ..Default::default()
      },
    );

    assert_eq!(
      columns1[2],
      ColumnInformationSchema {
        table_catalog: "template1".to_string(),
        table_schema: "public".to_string(),
        table_name: "table1".to_string(),
        column_name: "fk1".to_string(),
        ordinal_position: 3,
        is_nullable: "NO".to_string(),
        data_type: "integer".to_string(),
        column_default: None,
        is_generated: "NEVER".to_string(),
        primary_key: None,
        foreign_key: Some("table1_fk1_fkey".to_string()),
        unique_constraint: None,
        check_constraint: None,
        fk_table_name: Some("table0".to_string()),
        fk_column_name: Some("id".to_string()),
        ..Default::default()
      },
    );

    assert_eq!(
      columns1[3],
      ColumnInformationSchema {
        table_catalog: "template1".to_string(),
        table_schema: "public".to_string(),
        table_name: "table1".to_string(),
        column_name: "a".to_string(),
        ordinal_position: 4,
        is_nullable: "YES".to_string(),
        data_type: "text".to_string(),
        column_default: Some("'foo'::text".to_string()),
        is_generated: "NEVER".to_string(),
        unique_constraint: Some("table1_a_key".to_string()),
        check_constraint: Some("table1_a_check".to_string()),
        check_clause: Some("(a <> ''::text)".to_string()),
        ..Default::default()
      },
    );

    assert_eq!(
      columns1[4],
      ColumnInformationSchema {
        table_catalog: "template1".to_string(),
        table_schema: "public".to_string(),
        table_name: "table1".to_string(),
        column_name: "b".to_string(),
        ordinal_position: 5,
        is_nullable: "NO".to_string(),
        data_type: "bigint".to_string(),
        column_default: Some("5".to_string()),
        is_generated: "NEVER".to_string(),
        ..Default::default()
      },
    );
  }

  #[tokio::test]
  async fn postgres_table_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE TABLE table0 (
          id     INTEGER PRIMARY KEY NOT NULL,
          data   TEXT
        );

        CREATE TABLE table1 (
          id     INTEGER PRIMARY KEY,
          fk     INTEGER REFERENCES table0(id),
          a      TEXT DEFAULT ('foo'),
          b      INT8 NOT NULL DEFAULT (5)
        );

        CREATE TABLE table2 (
          id     INTEGER PRIMARY KEY REFERENCES table0(id)
        );
      ",
      )
      .await
      .unwrap();

    let table_schemas: Vec<Table> = conn
      .call_writer(|mut conn| {
        return build_all_table_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(3, table_schemas.len());

    let table1 = table_schemas
      .iter()
      .find(|t| t.name.name == "table1")
      .unwrap();

    assert_eq!(4, table1.columns.len());

    let table2 = table_schemas
      .iter()
      .find(|t| t.name.name == "table2")
      .unwrap();

    assert_eq!(1, table2.columns.len());
  }

  #[tokio::test]
  async fn postgres_check_jsonschema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE FUNCTION jsonschema(n TEXT, contents JSONB) RETURNS BOOL AS $$
          BEGIN
            CASE n
              -- We're lying a little here:
              WHEN 'std.FileUpload' THEN
                RETURN TRUE;
              WHEN 'std.FileUploads' THEN
                RETURN TRUE;
              ELSE
                RAISE EXCEPTION 'Not supported alongside PG';
            END CASE;
          END;
        $$ LANGUAGE plpgsql;
        ",
      )
      .await
      .unwrap();

    conn
      .execute_batch(
        "
        CREATE TABLE table0 (
          \"id\"       SERIAL PRIMARY KEY CHECK(id >= 0),
          \"file\"     JSONB CHECK(jsonschema('std.FileUpload', file)),
          \"files\"    JSONB CHECK(jsonschema('std.FileUploads', files))
        );
        ",
      )
      .await
      .unwrap();

    let table_schemas: Vec<Table> = conn
      .call_writer(|mut conn| {
        return build_all_table_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(1, table_schemas.len());
    let table = &table_schemas[0];
    assert_eq!("table0", table.name.name);

    let file_col = &table.columns[1];
    let file_check = file_col
      .options
      .iter()
      .filter_map(|o| match o {
        ColumnOption::Check(exp) => Some(exp),
        _ => None,
      })
      .collect::<Vec<_>>()[0];

    assert_eq!("jsonschema('std.FileUpload'::text, file)", file_check);

    let files_col = &table.columns[2];
    let files_check = files_col
      .options
      .iter()
      .filter_map(|o| match o {
        ColumnOption::Check(exp) => Some(exp),
        _ => None,
      })
      .collect::<Vec<_>>()[0];

    assert_eq!("jsonschema('std.FileUploads'::text, files)", files_check);
  }
}
