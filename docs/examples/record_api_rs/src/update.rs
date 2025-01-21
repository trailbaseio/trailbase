use trailbase_client::{Client, RecordId};

pub async fn update(client: &Client, id: impl RecordId<'_>) -> anyhow::Result<()> {
  Ok(
    client
      .records("simple_strict_table")
      .update(
        id,
        serde_json::json!({
            "text_not_null": "updated",
        }),
      )
      .await?,
  )
}
