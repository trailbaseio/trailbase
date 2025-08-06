use fallible_iterator::FallibleIterator;
use log::*;
use sqlite3_parser::ast::{Cmd, Stmt};
use sqlite3_parser::lexer::sql::{Error as Sqlite3Error, Parser};

pub fn parse_into_statements(sql: &str) -> Result<Vec<Stmt>, Sqlite3Error> {
  // According to sqlite3_parser's docs they're working to remove panics in some edge cases.
  // Meanwhile we'll trap them here. We haven't seen any in practice yet.
  let outer_result = std::panic::catch_unwind(|| {
    let mut parser = Parser::new(sql.as_bytes());

    let mut statements: Vec<Stmt> = vec![];
    while let Some(cmd) = parser.next()? {
      match cmd {
        Cmd::Stmt(stmt) => {
          statements.push(stmt);
        }
        Cmd::Explain(_) | Cmd::ExplainQueryPlan(_) => {}
      }
    }
    return Ok(statements);
  });

  return match outer_result {
    Ok(inner_result) => inner_result,
    Err(_panic_err) => {
      error!("Parser panicked");
      return Err(Sqlite3Error::UnrecognizedToken(None));
    }
  };
}

pub fn parse_into_statement(sql: &str) -> Result<Option<Stmt>, Sqlite3Error> {
  // According to sqlite3_parser's docs they're working to remove panics in some edge cases.
  // Meanwhile we'll trap them here. We haven't seen any in practice yet.
  let outer_result = std::panic::catch_unwind(|| {
    let mut parser = Parser::new(sql.as_bytes());

    while let Some(cmd) = parser.next()? {
      match cmd {
        Cmd::Stmt(stmt) => {
          return Ok(Some(stmt));
        }
        Cmd::Explain(_) | Cmd::ExplainQueryPlan(_) => {}
      }
    }
    return Ok(None);
  });

  return match outer_result {
    Ok(inner_result) => inner_result,
    Err(_panic_err) => {
      error!("Parser panicked");
      return Err(Sqlite3Error::UnrecognizedToken(None));
    }
  };
}
