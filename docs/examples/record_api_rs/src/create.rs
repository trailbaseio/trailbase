use trailbase_client::Client;

pub async fn create(client: &Client) -> anyhow::Result<String> {
  Ok(
    client
      .records("simple_strict_table")
      .create(serde_json::json!({
          "text_not_null": "test",
      }))
      .await?,
  )
}
