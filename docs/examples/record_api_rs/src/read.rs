use trailbase_client::{Client, RecordId};

pub async fn read(client: &Client, id: impl RecordId<'_>) -> anyhow::Result<serde_json::Value> {
  Ok(client.records("simple_strict_table").read(id).await?)
}
