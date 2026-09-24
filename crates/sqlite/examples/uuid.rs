use trailbase_sqlite::{Connection, Error};

#[derive(Debug)]
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
    .read_query_row("SELECT title, body FROM articles LIMIT 1", ())
    .await
    .unwrap()
    .map(|row| -> Result<Article, Error> {
      Ok(Article {
        title: row.get(0)?,
        body: row.get(1)?,
      })
    })
    .transpose()
    .unwrap();

  println!("Done! {article:?}");
}
