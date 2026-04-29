use rusqlite::fallible_iterator::FallibleIterator;
use std::sync::Arc;

use super::util::{columns, from_row};
use crate::error::Error;
use crate::rows::{Column, Rows};
use crate::sqlite::connection::Connection;

/// Batch execute SQL statements and return rows of last statement.
///
/// NOTE: This is a weird batch flavor that returns the last statement's rows. Most clients don't
/// support this. We thus keep it out of `Connection::execute_batch()`. We currently only rely on
/// it in the admin dashboard. Avoid further adoption.
pub async fn execute_batch(
  conn: &Connection,
  sql: impl AsRef<str> + Send + 'static,
) -> Result<Option<Rows>, Error> {
  return conn
    .exec
    .call_writer(
      move |conn: &mut rusqlite::Connection| -> Result<Option<Rows>, Error> {
        let batch = rusqlite::Batch::new(conn, sql.as_ref());

        let mut p = batch.peekable();
        while let Some(mut stmt) = p.next()? {
          let mut rows = stmt.raw_query();
          let row = rows.next()?;

          match p.peek()? {
            Some(_) => {}
            None => {
              if let Some(row) = row {
                let cols: Arc<Vec<Column>> = Arc::new(columns(row.as_ref()));

                let mut result = vec![from_row(row, cols.clone())?];
                while let Some(row) = rows.next()? {
                  result.push(from_row(row, cols.clone())?);
                }
                return Ok(Some(Rows(result, cols)));
              }

              return Ok(None);
            }
          }
        }

        return Ok(None);
      },
    )
    .await;
}
