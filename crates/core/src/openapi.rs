use utoipa::openapi::{ContactBuilder, InfoBuilder, LicenseBuilder, OpenApi, OpenApiBuilder};
use utoipa_axum::router::OpenApiRouter;

use crate::config::proto;
use crate::constants::ADMIN_API_PATH;

fn version() -> String {
  let version_info = trailbase_build::get_version_info!();
  return version_info
    .git_version()
    .map(|v| {
      let tag = v.tag();
      if let Some(commits_since) = v.commits_since {
        return format!("{tag} ({commits_since})");
      }
      return tag;
    })
    .unwrap_or_default();
}

pub(crate) fn add_info(openapi: OpenApi) -> OpenApi {
  const LICENSE: &str = "OSL-3.0";
  return OpenApiBuilder::new()
    .info(
      InfoBuilder::new()
        .title("TrailBase")
        .description(Some("OpenApi definitions of TrailBase's APIs"))
        .contact(Some(
          ContactBuilder::new()
            .email(Some("contact@trailbase.io"))
            .build(),
        ))
        .license(Some(
          LicenseBuilder::new()
            .name(LICENSE)
            .identifier(Some(LICENSE))
            .build(),
        ))
        .version(version())
        .build(),
    )
    .build()
    .merge_from(openapi);
}

// Initializes routes from fully initialized TrailBase. This would allow to even pick up routes
// from registered WASM components.
pub fn build_api_definitions_from_config(
  config: &proto::Config,
  connection_type: trailbase_sqlite::ConnectionType,
  include_admin: bool,
) -> utoipa::openapi::OpenApi {
  let custom_routers = if include_admin {
    vec![OpenApiRouter::new().nest(&format!("/{ADMIN_API_PATH}/"), crate::admin::router())]
  } else {
    vec![]
  };

  return add_info(
    crate::server::Server::build_main_router(
      config,
      connection_type,
      None,
      false,
      custom_routers,
      /* auth_rate_limit= */ None,
    )
    .unwrap_or_else(|err| {
      log::error!("failed to build main_router: {err}");

      return OpenApiRouter::new();
    })
    .into_openapi(),
  );
}
