use trailbase_client::{Client, DbEvent, RecordId, Stream};

pub async fn subscribe(
  client: &Client,
  id: impl RecordId<'_>,
) -> anyhow::Result<impl Stream<Item = DbEvent>> {
  Ok(client.records("simple_strict_table").subscribe(id).await?)
}

pub async fn subscribe_all(client: &Client) -> anyhow::Result<impl Stream<Item = DbEvent>> {
  Ok(client.records("simple_strict_table").subscribe("*").await?)
}
