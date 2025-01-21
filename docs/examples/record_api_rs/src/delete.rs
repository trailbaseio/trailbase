use trailbase_client::{Client, RecordId};

pub async fn delete(client: &Client, id: impl RecordId<'_>) -> anyhow::Result<()> {
  Ok(client.records("simple_strict_table").delete(id).await?)
}
