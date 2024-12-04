use axum::{
  extract::{Json, Path, RawQuery, State},
  routing::get,
  Router,
};
use base64::prelude::*;
use std::collections::HashMap;

pub mod error;
pub mod query_api;

pub use error::QueryError;
pub use query_api::QueryApi;

use crate::auth::User;
use crate::config::proto::QueryApiParameterType;
use crate::records::sql_to_json::rows_to_json_arrays;
use crate::AppState;

pub(crate) fn router() -> Router<AppState> {
  return Router::new().route("/:name", get(query_handler));
}

pub async fn query_handler(
  State(state): State<AppState>,
  Path(api_name): Path<String>,
  RawQuery(query): RawQuery,
  user: Option<User>,
) -> Result<Json<serde_json::Value>, QueryError> {
  use QueryError as E;

  let Some(api) = state.lookup_query_api(&api_name) else {
    return Err(E::ApiNotFound);
  };
  let virtual_table_name = api.virtual_table_name();

  let mut query_params: HashMap<String, String> = match query {
    Some(ref query) => form_urlencoded::parse(query.as_bytes())
      .map(|(k, v)| (k.to_string(), v.to_string()))
      .collect(),
    None => HashMap::new(),
  };

  let mut params: Vec<(String, trailbase_sqlite::Value)> = vec![];
  for (name, typ) in api.params() {
    match query_params.remove(name) {
      Some(value) => match *typ {
        QueryApiParameterType::Text => {
          params.push((
            format!(":{name}"),
            trailbase_sqlite::Value::Text(value.clone()),
          ));
        }
        QueryApiParameterType::Blob => {
          params.push((
            format!(":{name}"),
            trailbase_sqlite::Value::Blob(
              BASE64_URL_SAFE
                .decode(value)
                .map_err(|_err| E::BadRequest("not b64"))?,
            ),
          ));
        }
        QueryApiParameterType::Real => {
          params.push((
            format!(":{name}"),
            trailbase_sqlite::Value::Real(
              value
                .parse::<f64>()
                .map_err(|_err| E::BadRequest("expected f64"))?,
            ),
          ));
        }
        QueryApiParameterType::Integer => {
          params.push((
            format!(":{name}"),
            trailbase_sqlite::Value::Integer(
              value
                .parse::<i64>()
                .map_err(|_err| E::BadRequest("expected i64"))?,
            ),
          ));
        }
      },
      None => {
        params.push((format!(":{name}"), trailbase_sqlite::Value::Null));
      }
    };
  }

  if !query_params.is_empty() {
    return Err(E::BadRequest("invalid query param"));
  }

  api.check_api_access(&params, user.as_ref()).await?;

  const LIMIT: usize = 128;
  let response_rows = state
    .conn()
    .query(
      &format!(
        "SELECT * FROM {virtual_table_name}({placeholders}) WHERE TRUE LIMIT {LIMIT}",
        placeholders = params
          .iter()
          .map(|e| e.0.as_str())
          .collect::<Vec<_>>()
          .join(", ")
      ),
      params,
    )
    .await?;

  let (json_rows, columns) =
    rows_to_json_arrays(response_rows, LIMIT).map_err(|err| E::Internal(err.into()))?;

  let Some(columns) = columns else {
    return Err(E::Internal("Missing column mapping".into()));
  };

  // Turn the list of lists into an array of row-objects.
  let rows = serde_json::Value::Array(
    json_rows
      .into_iter()
      .map(|row| {
        return serde_json::Value::Object(
          row
            .into_iter()
            .enumerate()
            .map(|(idx, value)| (columns.get(idx).unwrap().name.clone(), value))
            .collect(),
        );
      })
      .collect(),
  );

  return Ok(Json(rows));
}

#[cfg(test)]
mod test {
  use super::*;
  use axum::extract::{Json, Path, RawQuery, State};

  use crate::app_state::*;
  use crate::config::proto::{
    QueryApiAcl, QueryApiConfig, QueryApiParameter, QueryApiParameterType,
  };

  #[tokio::test]
  async fn test_query_api() {
    let state = test_state(None).await.unwrap();

    let conn = state.conn();
    conn
      .execute(
        "CREATE VIRTUAL TABLE test_vtable USING define((SELECT $1 AS value))",
        (),
      )
      .await
      .unwrap();

    let mut config = state.get_config();
    config.query_apis.push(QueryApiConfig {
      name: Some("test".to_string()),
      virtual_table_name: Some("test_vtable".to_string()),
      params: vec![QueryApiParameter {
        name: Some("param0".to_string()),
        r#type: Some(QueryApiParameterType::Text.into()),
      }],
      acl: Some(QueryApiAcl::World.into()),
      access_rule: None,
    });
    state
      .validate_and_update_config(config, None)
      .await
      .unwrap();

    let Json(response) = query_handler(
      State(state),
      Path("test".to_string()),
      RawQuery(Some(r#"param0=test_param"#.to_string())),
      None,
    )
    .await
    .unwrap();

    assert_eq!(
      response,
      serde_json::json!([{
        "value": "test_param"
      }])
    );
  }
}
