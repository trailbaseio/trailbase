use trailbase_client::{Client, CompareOp, Filter, ListArguments, ListResponse, Pagination};

pub async fn list(client: &Client) -> anyhow::Result<ListResponse<serde_json::Value>> {
  Ok(
    client
      .records("movies")
      .list(
        ListArguments::new()
          .with_pagination(Pagination::new().with_limit(3))
          .with_order(["rank"])
          .with_filters([
            Filter::new("watch_time", CompareOp::LessThan, "120"),
            Filter::new("description", CompareOp::Like, "%love%"),
          ]),
      )
      .await?,
  )
}
