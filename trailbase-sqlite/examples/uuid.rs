use libsql::{params, Value::Text};
use trailbase_sqlite::connect_sqlite;

// NOTE: This binary demonstrates calling statically linked extensions, i.e. uuid_v7().
// NOTE: It also shows that libsql and sqlite_loadable can both be linked into the same binary
// despite both pulling in sqlite3 symbols through libsql-ffi and sqlite3ext-sys, respectively.
// Wasn't able to reproduce this in a larger binary :shrug:.

#[tokio::main]
async fn main() {
  let conn = connect_sqlite(None, None).await.unwrap();

  conn
    .query("SELECT 1", params!(Text("FOO".to_string())))
    .await
    .unwrap();

  let uuid = conn
    .prepare("SELECT (uuid_v7_text())")
    .await
    .unwrap()
    .query_row(())
    .await
    .unwrap();

  println!("Done! {uuid:?}");
}
