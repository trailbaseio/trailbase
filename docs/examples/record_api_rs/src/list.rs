use trailbase_client::{Client, ListArguments, ListResponse, Pagination};

pub async fn list(client: &Client) -> anyhow::Result<ListResponse<serde_json::Value>> {
  Ok(
    client
      .records("movies")
      .list(ListArguments {
        pagination: Pagination {
          limit: Some(3),
          cursor: None,
        },
        order: Some(&["rank"]),
        filters: Some(&["watch_time[lt]=120", "description[like]=%love%"]),
        ..Default::default()
      })
      .await?,
  )
}
