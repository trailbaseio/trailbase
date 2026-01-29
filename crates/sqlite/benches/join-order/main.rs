use chrono::DateTime;
use chrono::offset::Utc;
use csv::StringRecord;
use glob::glob;
use itertools::Itertools;
use rusqlite::types::Value;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Query {
  id: u64,
  variant: String,
}

impl Query {
  fn parse(s: &str) -> Self {
    let (id, variant) = s.split_at(s.len() - 1);
    return Self {
      id: id.parse().unwrap(),
      variant: variant.to_string(),
    };
  }
}

impl std::fmt::Display for Query {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    return write!(f, "{}{}", self.id, self.variant);
  }
}

fn import(conn: &rusqlite::Connection, base: &std::path::Path) {
  // Apply schema.
  let schema = std::fs::read_to_string(base.join("schema/schema.sql")).unwrap();
  conn.execute_batch(&schema).unwrap();

  let table_cols = table_columns();
  let int_cols = int_columns();

  // Insert data.
  for entry in glob(&base.join("data/*.csv").to_string_lossy()).unwrap() {
    let path = entry.unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    println!("importing: {path:?}");

    let cols = table_cols.get(stem.as_str()).unwrap();
    let int_cols: HashSet<String> = HashSet::from_iter(
      int_cols
        .get(stem.as_str())
        .unwrap()
        .iter()
        .map(|s| s.to_string()),
    );

    let column_names = cols.join(",");
    let placeholders = (0..cols.len()).map(|i| format!("?{}", i + 1)).join(",");
    let query = format!("INSERT INTO {stem} ({column_names}) VALUES ({placeholders})");

    let mut rdr = {
      let f = std::fs::File::open(path).unwrap();
      let mut rdr = csv::ReaderBuilder::new()
        .double_quote(true)
        .escape(Some(b'\\'))
        .from_reader(f);

      rdr.set_headers(StringRecord::from(cols.clone()));
      rdr
    };

    for result in rdr.records() {
      let Ok(record) = result else {
        println!("skipping '{stem}': {result:?}");
        continue;
      };

      assert_eq!(record.len(), cols.len());
      let params = cols.iter().enumerate().map(|(i, col)| {
        let v = &record[i];
        if v == "" {
          return Value::Null;
        }

        return if int_cols.contains(*col) {
          Value::Integer(v.parse().unwrap())
        } else {
          Value::Text(v.to_string())
        };
      });

      conn
        .execute(&query, rusqlite::params_from_iter(params))
        .unwrap();
    }
  }

  // Build indexes.
  let indexes = std::fs::read_to_string(base.join("schema/fkindexes.sql")).unwrap();
  conn.execute_batch(&indexes).unwrap();
}

fn main() {
  let base = PathBuf::from("./benches/join-order");
  // let tmp_dir = tempfile::TempDir::new().unwrap();
  // let fname = tmp_dir.path().join("db.sql");
  let fname = base.join("join_order.sql");
  let exists = std::fs::exists(&fname).unwrap_or(false);

  let conn = rusqlite::Connection::open(&fname).unwrap();

  // Setup connection.
  let pragmas = vec![
    ("busy_timeout", Value::Integer(10000)),
    ("journal_mode", Value::Text("WAL".into())),
    ("mmap_size", Value::Integer(1073741824)),
    ("synchronous", Value::Text("NORMAL".into())),
    ("temp_store", Value::Text("MEMORY".into())),
    // Safety feature around application-defined functions recommended by
    // https://sqlite.org/appfunc.html
    ("trusted_schema", Value::Text("OFF".into())),
    // Important improvement
    ("case_sensitive_like", Value::Text("ON".into())),
  ];

  for (name, v) in &pragmas {
    conn.pragma_update(None, name, v).unwrap();
  }

  if !exists {
    import(&conn, &base);
  }

  // Queries
  let mut queries: Vec<(Query, String)> = glob(&base.join("queries/*.sql").to_string_lossy())
    .unwrap()
    .map(|entry| {
      let path = entry.unwrap();
      let stmt = std::fs::read_to_string(&path).unwrap();

      return (
        Query::parse(&path.file_stem().unwrap().to_string_lossy()),
        stmt,
      );
    })
    .collect();

  queries.sort();

  let version: String = conn
    .query_one("SELECT sqlite_version()", (), |row| row.get(0))
    .unwrap();

  let mut output = {
    let now = SystemTime::now();
    let datetime: DateTime<Utc> = now.into();

    let mut output = std::fs::OpenOptions::new()
      .write(true)
      .create(true)
      .truncate(true)
      .open(base.join(format!(
        "results_sqlite_v{version}_{}.csv",
        datetime.date_naive().to_string()
      )))
      .unwrap();

    writeln!(&mut output, "# Timestamp: {}", datetime.to_rfc2822()).unwrap();
    writeln!(&mut output, "# SQLite: v{version}").unwrap();
    for (name, v) in &pragmas {
      writeln!(&mut output, "# PRAGMA {name}={v:?}").unwrap();
    }
    writeln!(&mut output, "#\n# name, iter, time (ms)").unwrap();

    output
  };

  // Optimize.
  conn.execute("PRAGMA optimize", ()).unwrap();

  for (query, stmt) in queries {
    println!("running {query}");

    let mut stmt = conn.prepare_cached(&stmt).unwrap();

    for i in 0..2 {
      let start = SystemTime::now();

      let mut rows = stmt.query(()).unwrap();
      let _row = rows.next().unwrap();

      let elapsed = SystemTime::now()
        .duration_since(start)
        .unwrap_or_default()
        .as_millis();

      println!("\ttook {elapsed}ms");
      writeln!(&mut output, "{query}, {i}, {elapsed}").unwrap();
    }
  }
}

fn table_columns() -> HashMap<&'static str, Vec<&'static str>> {
  return HashMap::from([
    (
      "aka_name",
      vec![
        "id",
        "person_id",
        "name",
        "imdb_index",
        "name_pcode_cf",
        "name_pcode_nf",
        "surname_pcode",
        "md5sum",
      ],
    ),
    (
      "aka_title",
      vec![
        "id",
        "movie_id",
        "title",
        "imdb_index",
        "kind_id",
        "production_year",
        "phonetic_code",
        "episode_of_id",
        "season_nr",
        "episode_nr",
        "note",
        "md5sum",
      ],
    ),
    (
      "cast_info",
      vec![
        "id",
        "person_id",
        "movie_id",
        "person_role_id",
        "note",
        "nr_order",
        "role_id",
      ],
    ),
    (
      "char_name",
      vec![
        "id",
        "name",
        "imdb_index",
        "imdb_id",
        "name_pcode_nf",
        "surname_pcode",
        "md5sum",
      ],
    ),
    ("comp_cast_type", vec!["id", "kind"]),
    (
      "company_name",
      vec![
        "id",
        "name",
        "country_code",
        "imdb_id",
        "name_pcode_nf",
        "name_pcode_sf",
        "md5sum",
      ],
    ),
    ("company_type", vec!["id", "kind"]),
    (
      "complete_cast",
      vec!["id", "movie_id", "subject_id", "status_id"],
    ),
    ("info_type", vec!["id", "info"]),
    ("keyword", vec!["id", "keyword", "phonetic_code"]),
    ("kind_type", vec!["id", "kind"]),
    ("link_type", vec!["id", "link"]),
    (
      "movie_companies",
      vec!["id", "movie_id", "company_id", "company_type_id", "note"],
    ),
    (
      "movie_info",
      vec!["id", "movie_id", "info_type_id", "info", "note"],
    ),
    (
      "movie_info_idx",
      vec!["id", "movie_id", "info_type_id", "info", "note"],
    ),
    ("movie_keyword", vec!["id", "movie_id", "keyword_id"]),
    (
      "movie_link",
      vec!["id", "movie_id", "linked_movie_id", "link_type_id"],
    ),
    (
      "name",
      vec![
        "id",
        "name",
        "imdb_index",
        "imdb_id",
        "gender",
        "name_pcode_cf",
        "name_pcode_nf",
        "surname_pcode",
        "md5sum",
      ],
    ),
    (
      "person_info",
      vec!["id", "person_id", "info_type_id", "info", "note"],
    ),
    ("role_type", vec!["id", "role"]),
    (
      "title",
      vec![
        "id",
        "title",
        "imdb_index",
        "kind_id",
        "production_year",
        "imdb_id",
        "phonetic_code",
        "episode_of_id",
        "season_nr",
        "episode_nr",
        "series_years",
        "md5sum",
      ],
    ),
  ]);
}

fn int_columns() -> HashMap<&'static str, Vec<&'static str>> {
  return HashMap::from([
    ("aka_name", vec!["id", "person_id"]),
    (
      "aka_title",
      vec![
        "id",
        "movie_id",
        "kind_id",
        "production_year",
        "episode_of_id",
        "season_nr",
        "episode_nr",
      ],
    ),
    (
      "cast_info",
      vec![
        "id",
        "person_id",
        "movie_id",
        "person_role_id",
        "nr_order",
        "role_id",
      ],
    ),
    ("char_name", vec!["id", "imdb_id"]),
    ("comp_cast_type", vec!["id"]),
    ("company_name", vec!["id", "imdb_id"]),
    ("company_type", vec!["id"]),
    (
      "complete_cast",
      vec!["id", "movie_id", "subject_id", "status_id"],
    ),
    ("info_type", vec!["id"]),
    ("keyword", vec!["id"]),
    ("kind_type", vec!["id"]),
    ("link_type", vec!["id"]),
    (
      "movie_companies",
      vec!["id", "movie_id", "company_id", "company_type_id"],
    ),
    ("movie_info", vec!["id", "movie_id", "info_type_id"]),
    ("movie_info_idx", vec!["id", "movie_id", "info_type_id"]),
    ("movie_keyword", vec!["id", "movie_id", "keyword_id"]),
    (
      "movie_link",
      vec!["id", "movie_id", "linked_movie_id", "link_type_id"],
    ),
    ("name", vec!["id", "imdb_id"]),
    ("person_info", vec!["id", "person_id", "info_type_id"]),
    ("role_type", vec!["id"]),
    (
      "title",
      vec![
        "id",
        "kind_id",
        "production_year",
        "imdb_id",
        "episode_of_id",
        "season_nr",
        "episode_nr",
      ],
    ),
  ]);
}
