use trailbase_client::{Client, ListArguments, ListResponse, Pagination};

pub async fn list(client: &Client) -> anyhow::Result<ListResponse<serde_json::Value>> {
  Ok(
    client
      .records("movies")
      .list(
        ListArguments::new()
          .with_pagination(Pagination::new().with_limit(3))
          .with_order(["rank"])
          .with_filters(["watch_time[lt]=120", "description[like]=%love%"]),
      )
      .await?,
  )
}
