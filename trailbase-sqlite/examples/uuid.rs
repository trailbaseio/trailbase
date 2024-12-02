use trailbase_sqlite::connect_sqlite;

// NOTE: This binary demonstrates calling statically linked extensions, i.e. uuid_v7().
// NOTE: It also shows that libsql and sqlite_loadable can both be linked into the same binary
// despite both pulling in sqlite3 symbols through libsql-ffi and sqlite3ext-sys, respectively.
// Wasn't able to reproduce this in a larger binary :shrug:.

fn main() {
  let conn = connect_sqlite(None, None).unwrap();

  let mut stmt = conn.prepare("SELECT (uuid_v7_text())").unwrap();

  let uuid = stmt
    .query_row((), |row| -> rusqlite::Result<[u8; 16]> { row.get(0) })
    .unwrap();

  println!("Done! {uuid:?}");
}
