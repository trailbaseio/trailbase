use log::*;
use std::sync::Arc;

use crate::auth::User;
use crate::config::proto::{QueryApiAcl, QueryApiConfig, QueryApiParameterType};
use crate::query::QueryError;
use trailbase_sqlite::query_one_row;

#[derive(Clone)]
pub struct QueryApi {
  state: Arc<QueryApiState>,
}

struct QueryApiState {
  conn: libsql::Connection,

  api_name: String,
  virtual_table_name: String,
  params: Vec<(String, QueryApiParameterType)>,

  acl: Option<QueryApiAcl>,
  access_rule: Option<String>,
}

impl QueryApi {
  pub fn from(conn: libsql::Connection, config: QueryApiConfig) -> Result<Self, String> {
    return Ok(QueryApi {
      state: Arc::new(QueryApiState {
        conn,
        api_name: config.name.ok_or("Missing name".to_string())?,
        virtual_table_name: config
          .virtual_table_name
          .ok_or("Missing vtable name".to_string())?,
        params: config
          .params
          .iter()
          .filter_map(|a| {
            return match (&a.name, a.r#type) {
              (Some(name), Some(typ)) => {
                if let Ok(t) = typ.try_into() {
                  Some((name.clone(), t))
                } else {
                  None
                }
              }
              _ => None,
            };
          })
          .collect(),
        acl: config.acl.and_then(|acl| acl.try_into().ok()),
        access_rule: config.access_rule,
      }),
    });
  }

  #[inline]
  pub fn api_name(&self) -> &str {
    &self.state.api_name
  }

  #[inline]
  pub fn virtual_table_name(&self) -> &str {
    return &self.state.virtual_table_name;
  }

  #[inline]
  pub fn params(&self) -> &Vec<(String, QueryApiParameterType)> {
    return &self.state.params;
  }

  pub(crate) async fn check_api_access(
    &self,
    query_params: &[(String, libsql::Value)],
    user: Option<&User>,
  ) -> Result<(), QueryError> {
    let Some(acl) = self.state.acl else {
      return Err(QueryError::Forbidden);
    };

    'acl: {
      match acl {
        QueryApiAcl::Undefined => break 'acl,
        QueryApiAcl::World => {}
        QueryApiAcl::Authenticated => {
          if user.is_none() {
            break 'acl;
          }
        }
      };

      match self.state.access_rule {
        None => return Ok(()),
        Some(ref access_rule) => {
          let params_subquery = query_params
            .iter()
            .filter_map(|(placeholder, _value)| {
              let Some(name) = placeholder.strip_prefix(":") else {
                warn!("Malformed placeholder: {placeholder}");
                return None;
              };
              return Some(format!("{placeholder} AS {name}"));
            })
            .collect::<Vec<_>>()
            .join(", ");

          let access_query = format!(
            r#"
              SELECT
                ({access_rule})
              FROM
                (SELECT :__user_id AS id) AS _USER_,
                (SELECT {params_subquery}) AS _PARAMS_
            "#,
          );

          let mut params = query_params.to_vec();
          params.push((
            ":__user_id".to_string(),
            user.map_or(libsql::Value::Null, |u| libsql::Value::Blob(u.uuid.into())),
          ));

          let row = match query_one_row(
            &self.state.conn,
            &access_query,
            libsql::params::Params::Named(params),
          )
          .await
          {
            Ok(row) => row,
            Err(err) => {
              error!("Query API access query: '{access_query}' failed: {err}");
              break 'acl;
            }
          };

          let allowed: bool = row.get(0).unwrap_or_else(|err| {
            if cfg!(test) {
              panic!(
                "Query API access query returned NULL. Failing closed: '{access_query}'\n{err}"
              );
            }

            warn!("RLA query returned NULL. Failing closed: '{access_query}'\n{err}");
            false
          });

          if allowed {
            return Ok(());
          }
        }
      }
    }

    return Err(QueryError::Forbidden);
  }
}
