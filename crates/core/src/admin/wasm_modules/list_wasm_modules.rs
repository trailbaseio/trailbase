use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Deserialize, Serialize, TS)]
pub struct WasmModuleEntry {
  pub name: String,
  pub display_name: String,
  pub icon: Option<String>,
  pub config_path: Option<String>,
  pub description: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ListWasmModulesResponse {
  pub modules: Vec<WasmModuleEntry>,
}

fn build_entry(name: String, manifest: Option<&crate::app_state::WasmManifest>) -> WasmModuleEntry {
  return WasmModuleEntry {
    display_name: manifest
      .map(|m| m.display_name.clone())
      .unwrap_or_else(|| name.clone()),
    icon: manifest.and_then(|m| m.icon.clone()),
    config_path: manifest.and_then(|m| m.config_path.clone()),
    description: manifest.and_then(|m| m.description.clone()),
    name,
  };
}

pub async fn list_wasm_modules_handler(
  State(state): State<AppState>,
) -> Result<Json<ListWasmModulesResponse>, Error> {
  let mut names: Vec<String> = Vec::new();
  for rt in state.wasm_runtimes() {
    let path = rt.read().await.component_path().clone();
    let name = path
      .file_stem()
      .and_then(|s| s.to_str())
      .unwrap_or("unknown")
      .to_string();
    names.push(name);
  }

  let manifests = state.wasm_manifests().read().await;
  let modules = names
    .into_iter()
    .map(|name| build_entry(name.clone(), manifests.get(&name)))
    .collect();

  return Ok(Json(ListWasmModulesResponse { modules }));
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app_state::{WasmManifest, test_state};

  #[test]
  fn build_entry_without_manifest_uses_name_as_display_name() {
    let entry = build_entry("my_component".to_string(), None);
    assert_eq!(entry.name, "my_component");
    assert_eq!(entry.display_name, "my_component");
    assert!(entry.icon.is_none());
    assert!(entry.config_path.is_none());
  }

  #[test]
  fn build_entry_with_manifest_propagates_fields() {
    let manifest = WasmManifest {
      display_name: "My Component".to_string(),
      icon: Some("<svg/>".to_string()),
      config_path: Some("/_/admin/my/config".to_string()),
      description: Some("A test component".to_string()),
    };
    let entry = build_entry("my_component".to_string(), Some(&manifest));
    assert_eq!(entry.name, "my_component");
    assert_eq!(entry.display_name, "My Component");
    assert_eq!(entry.icon.as_deref(), Some("<svg/>"));
    assert_eq!(entry.config_path.as_deref(), Some("/_/admin/my/config"));
    assert_eq!(entry.description.as_deref(), Some("A test component"));
  }

  #[tokio::test]
  async fn list_wasm_modules_handler_returns_empty_when_no_runtimes() {
    let state = test_state(None).await.unwrap();

    state.wasm_manifests().write().await.insert(
      "phantom".to_string(),
      WasmManifest {
        display_name: "Phantom".to_string(),
        icon: Some("<svg/>".to_string()),
        config_path: Some("/_/admin/phantom".to_string()),
        description: None,
      },
    );

    let response = list_wasm_modules_handler(State(state)).await.unwrap();
    assert!(response.modules.is_empty());
  }
}
