use axum::response::Response;

pub(crate) async fn unpack_json_response<T: for<'a> serde::Deserialize<'a>>(
  response: Response,
) -> Result<T, anyhow::Error> {
  let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await?;
  return Ok(serde_json::from_slice::<T>(&bytes)?);
}
