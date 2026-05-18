use serde::Deserialize;

use crate::error::Error;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TriggerInformationSchema {
  pub trigger_schema: String,
  pub trigger_name: String,
  pub event_manipulation: String,
  pub event_object_schema: String,
  pub event_object_table: String,
  pub action_statement: String,
  pub action_timing: String,
  pub action_orientation: String,
}

const QUERY_TRIGGERS: &str = "
SELECT
  trigger_schema,
  trigger_name,
  event_manipulation,  -- INSERT, UPDATE, DELETE
  event_object_schema,
  event_object_table,
  action_statement,
  action_timing,       -- BEFORE, AFTER, INSTEAD OF
  action_orientation   -- ROW or STATEMENT
FROM
  information_schema.triggers
WHERE
  trigger_schema NOT IN ('information_schema', 'pg_catalog')
ORDER BY
  event_object_table,
  trigger_name;
";

fn get_triggers(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<TriggerInformationSchema>, Error> {
  return conn
    .query_rows(QUERY_TRIGGERS, ())?
    .into_iter()
    .map(|row| {
      return Ok(TriggerInformationSchema {
        trigger_schema: row.get(0)?,
        trigger_name: row.get(1)?,
        event_manipulation: row.get(2)?,
        event_object_schema: row.get(3)?,
        event_object_table: row.get(4)?,
        action_statement: row.get(5)?,
        action_timing: row.get(6)?,
        action_orientation: row.get(7)?,
      });
    })
    .collect::<Result<_, Error>>();
}

pub fn build_all_trigger_schemas(
  conn: &mut impl trailbase_sqlite::SyncConnectionTrait,
) -> Result<Vec<TriggerInformationSchema>, Error> {
  return get_triggers(conn);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::util::test_connection;

  #[tokio::test]
  async fn postgres_trigger_schema_test() {
    let (_db, conn) = test_connection().await;

    conn
      .execute_batch(
        "
        CREATE OR REPLACE FUNCTION __identity_function() RETURNS TRIGGER AS $$
          BEGIN
            RETURN NEW;
          END;
        $$ LANGUAGE plpgsql;

        CREATE TABLE foo (id  INTEGER PRIMARY KEY);

        CREATE TRIGGER __foo_trigger BEFORE UPDATE ON foo
          FOR EACH ROW
          -- Prevent trigger triggering itself.
          WHEN (pg_trigger_depth() < 1)
          EXECUTE FUNCTION __identity_function();
        ",
      )
      .await
      .unwrap();

    let triggers = conn
      .call_writer(|mut conn| {
        return build_all_trigger_schemas(&mut conn)
          .map_err(|err| trailbase_sqlite::Error::Other(err.into()));
      })
      .await
      .unwrap();

    assert_eq!(1, triggers.len());
    assert_eq!("__foo_trigger", triggers[0].trigger_name);
  }
}
