/// This is a very simple binary demonstrating how TrailBase's SQLite extensions (e.g. uuid_v7)
/// can be used outside of TrailBase, thus avoiding lock-in.
use trailbase_sqlite::connect_sqlite;

fn main() {
  let conn = connect_sqlite(None, None).unwrap();

  let mut stmt = conn.prepare("SELECT (uuid_text(uuid_v7()))").unwrap();

  let uuid: String = stmt.query_row((), |row| row.get(0)).unwrap();

  println!("Done! {uuid:?}");
}
