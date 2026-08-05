use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use trailbase_wasm_common::manifest::Metadata;
use trailbase_wasm_component_repo::{
  download_component, install_wasm_component, remove_wasm_component, repo,
};
use ts_rs::TS;

use crate::admin::AdminError as Error;
use crate::app_state::AppState;

#[derive(Debug, Default, Deserialize, Serialize, TS)]
pub struct WasmComponent {
  // QUESTION: Should we remove name in favor of "path". The name is a simple derivative and we
  // can also chop on the client.
  pub name: String,
  pub path: String,
  #[ts(optional)]
  pub repo_id: Option<String>,

  /// Whether the component is loaded by the server. This is different from `installed`.
  pub loaded: bool,
  /// Whether the component is present ("installed") in the file-system. There may be skew with
  /// loaded, if the component was newly added/removed and a reload hasn't happened yet.
  pub installed: bool,

  // Below properties are manifest provided.
  #[ts(optional)]
  pub display_name: Option<String>,
  #[ts(optional)]
  pub description: Option<String>,
  #[ts(optional)]
  pub icon: Option<String>,
  #[ts(optional)]
  pub admin_ui_path: Option<String>,
  #[ts(optional)]
  pub guest_runtime: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub struct ListWasmComponentsResponse {
  pub components: Vec<WasmComponent>,
}

fn build_entry(
  filepath: &Path,
  metadata: Option<&Metadata>,
  repo_id: Option<String>,
  loaded: bool,
  installed: bool,
) -> Result<WasmComponent, Error> {
  let path = filepath.to_string_lossy().to_string();
  log::debug!("build_entry: {path}, {repo_id:?}");

  let name = trailbase_wasm_common::component_path_to_name(filepath)
    .map_err(|err| Error::Internal(err.into()))?;

  let Metadata {
    display_name,
    description,
    icon,
    admin_ui_path,
    guest_runtime,
  } = metadata.cloned().unwrap_or_default();

  return Ok(WasmComponent {
    name,
    path,
    loaded,
    installed,
    display_name,
    description,
    icon,
    admin_ui_path,
    guest_runtime: guest_runtime.map(|r| format!("{r:?}")),
    repo_id,
  });
}

pub async fn list_wasm_components_handler(
  State(state): State<AppState>,
) -> Result<Json<ListWasmComponentsResponse>, Error> {
  let depot_path = state.data_dir().root();

  let r = repo();
  let mut repo: Vec<_> = r.iter().collect();

  let mut components: Vec<WasmComponent> = vec![];

  for rt in state.wasm_runtimes() {
    let metadata_and_rt = rt.read().await;

    let filepath = metadata_and_rt.1.component_path();
    debug_assert!(filepath.is_relative());
    let installed = tokio::fs::try_exists(&filepath).await.unwrap_or(false);

    // We strip relative to depot, e.g. "wasm/foo.wasm".
    let Ok(path) = filepath.strip_prefix(depot_path) else {
      log::debug!("skip, invalid path: {filepath:?}");
      continue;
    };

    // Remove from repo if installed. Handle left-overs later.
    if let Some(filename) = path.file_name().map(|f| f.to_string_lossy())
      && let Some(first) = repo
        .extract_if(.., |c| c.files.iter().any(|f| *f == *filename))
        .next()
    {
      components.push(build_entry(
        path,
        metadata_and_rt.0.as_ref(),
        Some(first.id.clone()),
        /* loaded = */ true,
        installed,
      )?);
    } else {
      components.push(build_entry(
        path,
        metadata_and_rt.0.as_ref(),
        None,
        /* loaded = */ true,
        installed,
      )?);
    }
  }

  let base_path = PathBuf::from("wasm/");
  for not_loaded in repo {
    let Some(filename) = not_loaded.files.first() else {
      log::debug!("skip, missing filename: {not_loaded:?}");
      continue;
    };

    let path = base_path.join(filename);
    let installed = tokio::fs::try_exists(&path).await.unwrap_or(false);

    components.push(build_entry(
      &path,
      None,
      Some(not_loaded.id.clone()),
      /* loaded= */ false,
      installed,
    )?);
  }

  return Ok(Json(ListWasmComponentsResponse { components }));
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[ts(export)]
pub enum WasmComponentRequest {
  Path(String),
  RepoId(String),
}

pub async fn install_wasm_component_handler(
  State(state): State<AppState>,
  Json(request): Json<WasmComponentRequest>,
) -> Result<(), Error> {
  if state.demo_mode() {
    return Err(Error::Precondition(
      "Managing WASM components disallowed in demo".into(),
    ));
  }

  let id = match request {
    WasmComponentRequest::Path(_) => {
      return Err(Error::Precondition("repo id required".into()));
    }
    WasmComponentRequest::RepoId(id) => id,
  };

  let r = repo();
  let Some(component_def) = r.get(&id) else {
    return Err(Error::Precondition("component not found".into()));
  };

  let (url, bytes) = download_component(component_def)
    .await
    .map_err(Error::Internal)?;

  let filename = url.path();
  let paths = install_wasm_component(
    &state.data_dir().wasm_path(),
    filename,
    std::io::Cursor::new(bytes),
  )
  .await
  .map_err(Error::Internal)?;

  log::debug!("Installed components: {paths:?}");

  return Ok(());
}

pub async fn uninstall_wasm_component_handler(
  State(state): State<AppState>,
  Json(request): Json<WasmComponentRequest>,
) -> Result<(), Error> {
  if state.demo_mode() {
    return Err(Error::Precondition(
      "Managing WASM components disallowed in demo".into(),
    ));
  }

  let paths: Vec<PathBuf> = match request {
    WasmComponentRequest::Path(p) => vec![PathBuf::from(p)],
    WasmComponentRequest::RepoId(id) => {
      let r = repo();
      let Some(component_def) = r.get(&id) else {
        return Err(Error::Precondition("not found".into()));
      };

      let wasm_path = state.data_dir().wasm_path();

      component_def
        .files
        .iter()
        .map(|f| wasm_path.join(f))
        .collect()
    }
  };

  for component_path in paths {
    let path = state.data_dir().root().join(component_path);
    if let Err(err) = remove_wasm_component(&path).await {
      log::warn!("Failed to remove {path:?}: {err}");
    }
  }

  return Ok(());
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::PathBuf;

  #[test]
  fn build_entry_with_manifest_propagates_fields() {
    let manifest = Metadata {
      display_name: Some("My Component".to_string()),
      icon: Some("<svg/>".to_string()),
      admin_ui_path: Some("/_/admin/my/config".to_string()),
      description: Some("A test component".to_string()),
      ..Default::default()
    };

    let entry = build_entry(
      &PathBuf::from("wasm/my_component.wasm"),
      Some(&manifest),
      Some("repo_id".to_string()),
      true,
      true,
    )
    .unwrap();

    assert_eq!(entry.name, "my_component");
    assert_eq!(entry.path, "wasm/my_component.wasm");
    assert_eq!(entry.display_name.as_deref(), Some("My Component"));
    assert_eq!(entry.icon.as_deref(), Some("<svg/>"));
    assert_eq!(entry.admin_ui_path.as_deref(), Some("/_/admin/my/config"));
    assert_eq!(entry.description.as_deref(), Some("A test component"));
    assert_eq!(entry.repo_id.as_deref(), Some("repo_id"));
    assert!(entry.installed);
  }
}
