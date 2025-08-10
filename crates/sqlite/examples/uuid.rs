/// This is a very simple binary demonstrating how TrailBase's SQLite extensions (e.g. uuid_v7)
/// can be used outside of TrailBase, thus avoiding lock-in.
use trailbase_extension::connect_sqlite;
use trailbase_sqlite::Connection;

#[tokio::main]
async fn main() {
  let conn = Connection::new(|| connect_sqlite(None), None).expect("in memory connection");

  let uuid: Option<String> = conn
    .read_query_value("SELECT (uuid_text(uuid_v7()))", ())
    .await
    .unwrap();

  println!("Done! {uuid:?}");
}
