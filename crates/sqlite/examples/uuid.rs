use serde::Deserialize;
use trailbase_sqlite::Connection;

#[derive(Debug, Deserialize)]
pub struct Article {
  pub title: String,
  pub body: String,
}

#[tokio::main]
async fn main() {
  let conn = Connection::open_in_memory().unwrap();

  conn
    .execute_batch(
      "CREATE TABLE articles (
            id     INTEGER PRIMARY KEY,
            title  TEXT NOT NULL,
            body   TEXT NOT NULL
       ) STRICT;

       INSERT INTO articles (title, body) VALUES ('first', 'body');
      ",
    )
    .await
    .unwrap();

  let article: Option<Article> = conn
    .read_query_value("SELECT * FROM articles LIMIT 1", ())
    .await
    .unwrap();

  println!("Done! {article:?}");
}
