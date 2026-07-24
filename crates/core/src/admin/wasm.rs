use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use trailbase_wasm_common::manifest::Metadata;
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Default, Deserialize, Serialize, TS)]
pub struct WasmComponent {
  pub name: String,
  pub display_name: Option<String>,
  pub description: Option<String>,
  pub icon: Option<String>,
  pub admin_ui_path: Option<String>,
  pub guest_runtime: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ListWasmComponentsResponse {
  pub components: Vec<WasmComponent>,
}

fn build_entry(name: String, metadata: Option<&Metadata>) -> WasmComponent {
  if let Some(Metadata {
    display_name,
    description,
    icon,
    admin_ui_path,
    guest_runtime,
  }) = metadata.cloned()
  {
    return WasmComponent {
      name,
      display_name,
      description,
      icon,
      admin_ui_path,
      guest_runtime: guest_runtime.map(|r| format!("{r:?}")),
    };
  }
  return WasmComponent {
    name,
    ..Default::default()
  };
}

pub async fn list_wasm_components_handler(
  State(state): State<AppState>,
) -> Result<Json<ListWasmComponentsResponse>, Error> {
  let mut components: Vec<WasmComponent> = vec![];
  for rt in state.wasm_runtimes() {
    let metadata_and_rt = rt.read().await;
    let name = metadata_and_rt
      .1
      .component_path()
      .file_stem()
      .and_then(|s| s.to_str())
      .unwrap_or("unknown")
      .to_string();

    components.push(build_entry(name, metadata_and_rt.0.as_ref()));
  }

  return Ok(Json(ListWasmComponentsResponse { components }));
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn build_entry_without_manifest_uses_name_as_display_name() {
    let entry = build_entry("my_component".to_string(), None);
    assert_eq!(entry.name, "my_component");
    assert_eq!(None, entry.display_name);
    assert_eq!(None, entry.icon);
    assert_eq!(None, entry.admin_ui_path);
  }

  #[test]
  fn build_entry_with_manifest_propagates_fields() {
    let manifest = Metadata {
      display_name: Some("My Component".to_string()),
      icon: Some("<svg/>".to_string()),
      admin_ui_path: Some("/_/admin/my/config".to_string()),
      description: Some("A test component".to_string()),
      ..Default::default()
    };
    let entry = build_entry("my_component".to_string(), Some(&manifest));
    assert_eq!(entry.name, "my_component");
    assert_eq!(entry.display_name.as_deref(), Some("My Component"));
    assert_eq!(entry.icon.as_deref(), Some("<svg/>"));
    assert_eq!(entry.admin_ui_path.as_deref(), Some("/_/admin/my/config"));
    assert_eq!(entry.description.as_deref(), Some("A test component"));
  }
}
