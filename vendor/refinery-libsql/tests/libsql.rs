use refinery_libsql::LibsqlConnection;

use barrel::backend::Sqlite as Sql;
use libsql::{params, Builder, Row};
use refinery::{embed_migrations, error::Kind, AsyncMigrate, Migration, Target};
use time::OffsetDateTime;

const DEFAULT_TABLE_NAME: &str = "refinery_schema_history";

mod embedded {
  use refinery::embed_migrations;
  embed_migrations!("./tests/migrations");
}

mod broken {
  use refinery::embed_migrations;
  embed_migrations!("./tests/migrations_broken");
}

mod missing {
  use refinery::embed_migrations;
  embed_migrations!("./tests/migrations_missing");
}

fn get_migrations() -> Vec<Migration> {
  embed_migrations!("./tests/migrations");

  let migration1 =
    Migration::unapplied("V1__initial.rs", &migrations::V1__initial::migration()).unwrap();

  let migration2 = Migration::unapplied(
    "V2__add_cars_and_motos_table.sql",
    include_str!("./migrations/V1-2/V2__add_cars_and_motos_table.sql"),
  )
  .unwrap();

  let migration3 = Migration::unapplied(
    "V3__add_brand_to_cars_table",
    include_str!("./migrations/V3/V3__add_brand_to_cars_table.sql"),
  )
  .unwrap();

  let migration4 = Migration::unapplied(
    "V4__add_year_to_motos_table.rs",
    &migrations::V4__add_year_to_motos_table::migration(),
  )
  .unwrap();

  let migration5 = Migration::unapplied(
    "V5__add_year_field_to_cars",
    "ALTER TABLE cars ADD year INTEGER;",
  )
  .unwrap();

  vec![migration1, migration2, migration3, migration4, migration5]
}

async fn in_memory_conn() -> LibsqlConnection {
  let db = Builder::new_local(":memory:").build().await.unwrap();
  LibsqlConnection::from_connection(db.connect().unwrap())
}

async fn query_one(conn: &mut LibsqlConnection, sql: &str) -> Option<Row> {
  let mut rows = conn.query(&sql, params![]).await.unwrap();
  rows.next().await.unwrap()
}

#[tokio::test]
async fn report_contains_applied_migrations() {
  let mut conn = in_memory_conn().await;
  let report = embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migrations = get_migrations();
  let applied_migrations = report.applied_migrations();

  assert_eq!(4, applied_migrations.len());

  assert_eq!(migrations[0].version(), applied_migrations[0].version());
  assert_eq!(migrations[1].version(), applied_migrations[1].version());
  assert_eq!(migrations[2].version(), applied_migrations[2].version());
  assert_eq!(migrations[3].version(), applied_migrations[3].version());

  assert_eq!(migrations[0].name(), migrations[0].name());
  assert_eq!(migrations[1].name(), applied_migrations[1].name());
  assert_eq!(migrations[2].name(), applied_migrations[2].name());
  assert_eq!(migrations[3].name(), applied_migrations[3].name());

  assert_eq!(migrations[0].checksum(), applied_migrations[0].checksum());
  assert_eq!(migrations[1].checksum(), applied_migrations[1].checksum());
  assert_eq!(migrations[2].checksum(), applied_migrations[2].checksum());
  assert_eq!(migrations[3].checksum(), applied_migrations[3].checksum());
}

#[tokio::test]
async fn creates_migration_table() {
  let mut conn = in_memory_conn().await;
  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();
  let table_name: String = query_one(
    &mut conn,
    &format!(
      "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
      DEFAULT_TABLE_NAME
    ),
  )
  .await
  .unwrap()
  .get(0)
  .unwrap();
  assert_eq!(DEFAULT_TABLE_NAME, table_name);
}

#[tokio::test]
async fn creates_migration_table_grouped_transaction() {
  let mut conn = in_memory_conn().await;
  embedded::migrations::runner()
    .set_grouped(true)
    .run_async(&mut conn)
    .await
    .unwrap();

  let row = query_one(
    &mut conn,
    &format!(
      "SELECT name FROM sqlite_master WHERE type='table' AND name='{}'",
      DEFAULT_TABLE_NAME
    ),
  )
  .await
  .unwrap();

  let table_name: String = row.get(0).unwrap();
  assert_eq!(DEFAULT_TABLE_NAME, table_name);
}

#[tokio::test]
async fn applies_migration() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  conn
    .execute(
      "INSERT INTO persons (name, city) VALUES (?, ?)",
      ["John Legend", "New York"],
    )
    .await
    .unwrap();
  let row = query_one(&mut conn, "SELECT name, city FROM persons")
    .await
    .unwrap();
  let (name, city): (String, String) = (row.get(0).unwrap(), row.get(1).unwrap());
  assert_eq!("John Legend", name);
  assert_eq!("New York", city);
}

#[tokio::test]
async fn applies_migration_grouped_transaction() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .set_grouped(true)
    .run_async(&mut conn)
    .await
    .unwrap();

  conn
    .execute(
      "INSERT INTO persons (name, city) VALUES (?, ?)",
      ["John Legend", "New York"],
    )
    .await
    .unwrap();
  let row = query_one(&mut conn, "SELECT name, city FROM persons")
    .await
    .unwrap();
  let (name, city): (String, String) = (row.get(0).unwrap(), row.get(1).unwrap());
  assert_eq!("John Legend", name);
  assert_eq!("New York", city);
}

#[tokio::test]
async fn updates_schema_history() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  assert_eq!(4, current.version());

  assert_eq!(
    OffsetDateTime::now_utc().date(),
    current.applied_on().unwrap().date()
  );
}

#[tokio::test]
async fn updates_schema_history_grouped_transaction() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .set_grouped(true)
    .run_async(&mut conn)
    .await
    .unwrap();

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  assert_eq!(4, current.version());

  assert_eq!(
    OffsetDateTime::now_utc().date(),
    current.applied_on().unwrap().date()
  );
}

#[tokio::test]
async fn updates_to_last_working_if_not_grouped() {
  let mut conn = in_memory_conn().await;

  let result = broken::migrations::runner().run_async(&mut conn).await;

  assert!(result.is_err());
  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  let err = result.unwrap_err();
  let migrations = get_migrations();
  let applied_migrations = err.report().unwrap().applied_migrations();

  assert_eq!(
    OffsetDateTime::now_utc().date(),
    current.applied_on().unwrap().date()
  );
  assert_eq!(2, current.version());
  assert_eq!(2, applied_migrations.len());

  assert_eq!(1, applied_migrations[0].version());
  assert_eq!(2, applied_migrations[1].version());

  assert_eq!("initial", migrations[0].name());
  assert_eq!("add_cars_table", applied_migrations[1].name());

  assert_eq!(2959965718684201605, applied_migrations[0].checksum());
  assert_eq!(8238603820526370208, applied_migrations[1].checksum());
}

#[tokio::test]
async fn doesnt_update_to_last_working_if_grouped() {
  let mut conn = in_memory_conn().await;

  let result = broken::migrations::runner()
    .set_grouped(true)
    .run_async(&mut conn)
    .await;

  assert!(result.is_err());
  let row = query_one(&mut conn, "SELECT version FROM refinery_schema_history").await;
  assert!(row.is_none());
}

#[tokio::test]
async fn gets_applied_migrations() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migrations = get_migrations();
  let applied_migrations = conn
    .get_applied_migrations(DEFAULT_TABLE_NAME)
    .await
    .unwrap();
  assert_eq!(4, applied_migrations.len());

  assert_eq!(migrations[0].version(), applied_migrations[0].version());
  assert_eq!(migrations[1].version(), applied_migrations[1].version());
  assert_eq!(migrations[2].version(), applied_migrations[2].version());
  assert_eq!(migrations[3].version(), applied_migrations[3].version());

  assert_eq!(migrations[0].name(), migrations[0].name());
  assert_eq!(migrations[1].name(), applied_migrations[1].name());
  assert_eq!(migrations[2].name(), applied_migrations[2].name());
  assert_eq!(migrations[3].name(), applied_migrations[3].name());

  assert_eq!(migrations[0].checksum(), applied_migrations[0].checksum());
  assert_eq!(migrations[1].checksum(), applied_migrations[1].checksum());
  assert_eq!(migrations[2].checksum(), applied_migrations[2].checksum());
  assert_eq!(migrations[3].checksum(), applied_migrations[3].checksum());
}

#[tokio::test]
async fn applies_new_migration() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migrations = get_migrations();

  let mchecksum = migrations[4].checksum();
  conn
    .migrate(
      &migrations,
      true,
      true,
      false,
      Target::Latest,
      DEFAULT_TABLE_NAME,
    )
    .await
    .unwrap();

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  assert_eq!(5, current.version());
  assert_eq!(mchecksum, current.checksum());
}

#[tokio::test]
async fn migrates_to_target_migration() {
  let mut conn = in_memory_conn().await;

  let report = embedded::migrations::runner()
    .set_target(Target::Version(3))
    .run_async(&mut conn)
    .await
    .unwrap();

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  let applied_migrations = report.applied_migrations();
  let migrations = get_migrations();

  assert_eq!(3, current.version());

  assert_eq!(3, applied_migrations.len());

  assert_eq!(migrations[0].version(), applied_migrations[0].version());
  assert_eq!(migrations[1].version(), applied_migrations[1].version());
  assert_eq!(migrations[2].version(), applied_migrations[2].version());

  assert_eq!(migrations[0].name(), migrations[0].name());
  assert_eq!(migrations[1].name(), applied_migrations[1].name());
  assert_eq!(migrations[2].name(), applied_migrations[2].name());

  assert_eq!(migrations[0].checksum(), applied_migrations[0].checksum());
  assert_eq!(migrations[1].checksum(), applied_migrations[1].checksum());
  assert_eq!(migrations[2].checksum(), applied_migrations[2].checksum());
}

#[tokio::test]
async fn migrates_to_target_migration_grouped() {
  let mut conn = in_memory_conn().await;

  let report = embedded::migrations::runner()
    .set_target(Target::Version(3))
    .set_grouped(true)
    .run_async(&mut conn)
    .await
    .unwrap();

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();

  let applied_migrations = report.applied_migrations();
  let migrations = get_migrations();

  assert_eq!(3, current.version());

  assert_eq!(3, applied_migrations.len());

  assert_eq!(migrations[0].version(), applied_migrations[0].version());
  assert_eq!(migrations[1].version(), applied_migrations[1].version());
  assert_eq!(migrations[2].version(), applied_migrations[2].version());

  assert_eq!(migrations[0].name(), migrations[0].name());
  assert_eq!(migrations[1].name(), applied_migrations[1].name());
  assert_eq!(migrations[2].name(), applied_migrations[2].name());

  assert_eq!(migrations[0].checksum(), applied_migrations[0].checksum());
  assert_eq!(migrations[1].checksum(), applied_migrations[1].checksum());
  assert_eq!(migrations[2].checksum(), applied_migrations[2].checksum());
}

#[tokio::test]
async fn aborts_on_missing_migration_on_filesystem() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migration = Migration::unapplied(
    "V4__add_year_field_to_cars",
    "ALTER TABLE cars ADD year INTEGER;",
  )
  .unwrap();
  let err = conn
    .migrate(
      &[migration],
      true,
      true,
      false,
      Target::Latest,
      DEFAULT_TABLE_NAME,
    )
    .await
    .unwrap_err();

  match err.kind() {
    Kind::MissingVersion(missing) => {
      assert_eq!(1, missing.version());
      assert_eq!("initial", missing.name());
    }
    _ => panic!("failed test"),
  }
}

#[tokio::test]
async fn aborts_on_divergent_migration() {
  let mut conn = in_memory_conn().await;

  embedded::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migration = Migration::unapplied(
    "V2__add_year_field_to_cars",
    "ALTER TABLE cars ADD year INTEGER;",
  )
  .unwrap();
  let err = conn
    .migrate(
      &[migration.clone()],
      true,
      false,
      false,
      Target::Latest,
      DEFAULT_TABLE_NAME,
    )
    .await
    .unwrap_err();

  match err.kind() {
    Kind::DivergentVersion(applied, divergent) => {
      assert_eq!(&migration, divergent);
      assert_eq!(2, applied.version());
      assert_eq!("add_cars_and_motos_table", applied.name());
    }
    _ => panic!("failed test"),
  }
}

#[tokio::test]
async fn aborts_on_missing_migration_on_database() {
  let mut conn = in_memory_conn().await;

  missing::migrations::runner()
    .run_async(&mut conn)
    .await
    .unwrap();

  let migration1 = Migration::unapplied(
    "V1__initial",
    concat!(
      "CREATE TABLE persons (",
      "id int,",
      "name varchar(255),",
      "city varchar(255)",
      ");"
    ),
  )
  .unwrap();

  let migration2 = Migration::unapplied(
    "V2__add_cars_table",
    include_str!("./migrations_missing/V2__add_cars_table.sql"),
  )
  .unwrap();
  let err = conn
    .migrate(
      &[migration1, migration2],
      true,
      true,
      false,
      Target::Latest,
      DEFAULT_TABLE_NAME,
    )
    .await
    .unwrap_err();
  match err.kind() {
    Kind::MissingVersion(missing) => {
      assert_eq!(1, missing.version());
      assert_eq!("initial", missing.name());
    }
    _ => panic!("failed test"),
  }
}

#[tokio::test]
async fn doesnt_run_migrations_if_fake_version() {
  let mut conn = in_memory_conn().await;

  let report = embedded::migrations::runner()
    .set_target(Target::FakeVersion(2))
    .run_async(&mut conn)
    .await
    .unwrap();

  let applied_migrations = report.applied_migrations();

  assert!(applied_migrations.is_empty());

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();
  let migrations = get_migrations();
  let mchecksum = migrations[1].checksum();

  assert_eq!(2, current.version());
  assert_eq!(mchecksum, current.checksum());

  let row: Option<Row> = query_one(
    &mut conn,
    "SELECT name FROM sqlite_master WHERE type='table' AND name='persons'",
  )
  .await;

  assert!(matches!(row, None));
}

#[tokio::test]
async fn doesnt_run_migrations_if_fake() {
  let mut conn = in_memory_conn().await;

  let report = embedded::migrations::runner()
    .set_target(Target::Fake)
    .run_async(&mut conn)
    .await
    .unwrap();

  let applied_migrations = report.applied_migrations();

  assert!(applied_migrations.is_empty());

  let current = conn
    .get_last_applied_migration(DEFAULT_TABLE_NAME)
    .await
    .unwrap()
    .unwrap();
  let migrations = get_migrations();
  let mchecksum = migrations[3].checksum();

  assert_eq!(4, current.version());
  assert_eq!(mchecksum, current.checksum());

  let row: Option<Row> = query_one(
    &mut conn,
    "SELECT name FROM sqlite_master WHERE type='table' AND name='persons'",
  )
  .await;

  assert!(matches!(row, None));
}

// #[tokio::test]
// fn migrates_from_cli() {
//     run_test(|| {
//         Command::new("refinery")
//             .args([
//                 "migrate",
//                 "-c",
//                 "tests/sqlite_refinery.toml",
//                 "-p",
//                 "tests/migrations",
//             ])
//             .unwrap()
//             .assert()
//             .stdout(contains("applying migration: V2__add_cars_and_motos_table"))
//             .stdout(contains("applying migration: V3__add_brand_to_cars_table"));
//     })
// }
