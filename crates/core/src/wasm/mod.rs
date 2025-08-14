use axum::Router;
use trailbase_wasm::Runtime;

use crate::AppState;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

async fn install_routes_and_jobs(
  state: &AppState,
  runtime: &Runtime,
) -> Result<Option<Router<AppState>>, AnyError> {
  runtime
    .call(async |instance| {
      let _ = instance.call_init().await;
    })
    .await?;

  return Ok(None);
}
