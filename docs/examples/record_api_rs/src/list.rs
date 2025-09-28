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
            // Multiple filters on same column: watch_time between 90 and 120 minutes
            Filter::new("watch_time", CompareOp::GreaterThanOrEqual, "90"),
            Filter::new("watch_time", CompareOp::LessThan, "120"),
            // Date range: movies released between 2020 and 2023
            Filter::new("release_date", CompareOp::GreaterThanOrEqual, "2020-01-01"),
            Filter::new("release_date", CompareOp::LessThanOrEqual, "2023-12-31"),
          ]),
      )
      .await?,
  )
}
