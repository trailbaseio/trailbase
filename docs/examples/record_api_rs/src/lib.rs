pub mod create;
pub mod delete;
pub mod read;
pub mod subscribe;
pub mod update;

#[cfg(test)]
mod test {
  use futures::StreamExt;
  use trailbase_client::Client;

  use crate::create::*;
  use crate::delete::*;
  use crate::read::*;
  use crate::subscribe::*;
  use crate::update::*;

  async fn connect() -> Client {
    let client = Client::new("http://localhost:4000", None).unwrap();
    client.login("admin@localhost", "secret").await.unwrap();
    client
  }

  // CI should ignore this test, since it's not hermetic.
  #[ignore]
  #[tokio::test]
  async fn example_test() {
    let client = connect().await;

    let table_stream = subscribe_all(&client).await.unwrap();

    let id = create(&client).await.unwrap();

    let record_stream = subscribe(&client, id.clone()).await.unwrap();

    {
      let record = read(&client, id.clone()).await.unwrap();
      assert_eq!(
        record.get("text_not_null").unwrap(),
        &serde_json::Value::String("test".into())
      );
    }

    {
      update(&client, id.clone()).await.unwrap();
      let record = read(&client, id.clone()).await.unwrap();
      assert_eq!(
        record.get("text_not_null").unwrap(),
        &serde_json::Value::String("updated".into())
      );
    }

    delete(&client, id).await.unwrap();

    let record_events = record_stream.collect::<Vec<_>>().await;
    assert_eq!(record_events.len(), 2);

    let table_events = table_stream.take(3).collect::<Vec<_>>().await;
    assert_eq!(table_events.len(), 3);
  }
}
