use trailbase_client::{Client, ListResponse, Pagination};

pub async fn list(client: &Client) -> anyhow::Result<ListResponse<serde_json::Value>> {
  Ok(
    client
      .records("movies")
      .list(
        Pagination {
          limit: Some(3),
          ..Default::default()
        },
        &["rank"],
        &["watch_time[lt]=120", "description[like]=%love%"],
      )
      .await?,
  )
}
