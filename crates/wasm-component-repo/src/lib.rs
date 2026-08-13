#![forbid(unsafe_code, clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

use std::fs;
use std::io::{Read, Seek};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use trailbase_wasm_runtime_host::find_wasm_components;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug)]
pub struct ComponentDefinition {
  pub id: String,
  pub url_template: String,
  pub files: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ComponentRepository {
  repo: Arc<Vec<ComponentDefinition>>,
}

impl ComponentRepository {
  pub fn get(&self, id: &str) -> Option<&ComponentDefinition> {
    return self.repo.iter().find(|c| c.id == id);
  }

  pub fn iter(&self) -> std::slice::Iter<'_, ComponentDefinition> {
    return self.repo.iter();
  }
}

pub fn repo() -> ComponentRepository {
  // TODO: Eventually we may want to read the repo from somwhere.
  static REPO: LazyLock<ComponentRepository> = LazyLock::new(|| {
    return ComponentRepository {
      repo: Arc::new(vec![
          ComponentDefinition {
              id: "trailbase/auth_ui".to_string(),

              url_template: "https://github.com/trailbaseio/trailbase/releases/download/{{ release }}/trailbase_{{ release }}_wasm_auth_ui.zip".to_string(),
              files: vec![
                  "trailbase_auth_ui_component.wasm".to_string(),
                  // Old filename. We keep this here to be able to identify legacy installations
                  // for folks who haven't updated the component lately.
                  "auth_ui_component.wasm".to_string(),
              ],
          },
          ComponentDefinition {
              id: "trailbase/tetris".to_string(),
              url_template: "https://github.com/trailbaseio/trailbase/releases/download/{{ release }}/trailbase_{{ release }}_wasm_tetris.zip".to_string(),
              files: vec![
                  "trailbase_tetris_component.wasm".to_string(),
              ],
          },
       ]),
    };
  });

  return REPO.clone();
}

pub fn find_component(name: &str) -> Option<ComponentDefinition> {
  return repo().get(name).cloned();
}

pub fn find_component_by_filename(
  repo: &ComponentRepository,
  filename: &str,
) -> Option<ComponentDefinition> {
  return repo
    .iter()
    .find(|component_def| {
      return component_def.files.iter().any(|f| f.as_str() == filename);
    })
    .cloned();
}

pub async fn download_component(
  component_def: &ComponentDefinition,
) -> Result<(url::Url, bytes::Bytes), BoxError> {
  use minijinja::{Environment, context};

  let version = trailbase_build::get_version_info!();
  let Some(git_version) = version.git_version() else {
    return Err("missing version".into());
  };

  let env = Environment::empty();
  let url_str = env
    .template_from_named_str("url", &component_def.url_template)?
    .render(context! {
        release => git_version.tag(),
    })?;
  let url = url::Url::parse(&url_str)?;

  log::info!("Downloading {url}");

  let bytes = reqwest::get(url.clone())
    .await?
    .bytes()
    .await
    .map_err(|err| {
      log::error!("Failed to download {url}: {err}");
      return err;
    })?;

  return Ok((url, bytes));
}

pub async fn remove_wasm_component(component_path: &Path) -> Result<(), BoxError> {
  fs::remove_file(component_path)?;
  return Ok(());
}

pub async fn install_wasm_component(
  components_dir: &Path,
  path: impl AsRef<std::path::Path>,
  mut reader: impl Read + Seek,
) -> Result<Vec<std::path::PathBuf>, BoxError> {
  let path = path.as_ref();
  if !fs::exists(components_dir)? {
    fs::create_dir_all(components_dir)?;
  }

  return match path
    .extension()
    .map(|p| p.to_string_lossy().to_string())
    .as_deref()
  {
    Some("zip") => {
      let mut archive = zip::ZipArchive::new(reader)?;

      let mut paths: Vec<std::path::PathBuf> = vec![];
      for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if let Some(path) = file.enclosed_name() {
          if path.extension().and_then(|e| e.to_str()) != Some("wasm") {
            continue;
          }

          let Some(filename) = path.file_name().and_then(|e| e.to_str()) else {
            return Err(format!("Invalid filename: {:?}", file.name()).into());
          };
          let component_file_path = components_dir.join(filename);
          let mut component_file = std::fs::File::create(&component_file_path)?;
          std::io::copy(&mut file, &mut component_file)?;

          paths.push(component_file_path);
        }
      }

      Ok(paths)
    }
    Some("wasm") => {
      let Some(filename) = path.file_name().and_then(|e| e.to_str()) else {
        return Err(format!("Invalid filename: {path:?}").into());
      };

      let component_file_path = components_dir.join(filename);
      let mut component_file = std::fs::File::create(&component_file_path)?;
      std::io::copy(&mut reader, &mut component_file)?;

      Ok(vec![component_file_path])
    }
    _ => Err("unexpected format".into()),
  };
}

#[derive(serde::Serialize)]
pub struct Package {
  pub name: String,
  pub namespace: String,
  pub version: Option<String>,
  pub worlds: Vec<String>,
  pub interfaces: Vec<String>,
}

#[derive(serde::Serialize)]
pub struct Component {
  pub path: std::path::PathBuf,
  pub packages: Vec<Package>,
}

pub fn list_installed_wasm_components(components_dir: &Path) -> Result<Vec<Component>, BoxError> {
  let components: Vec<(Vec<u8>, std::path::PathBuf)> = find_wasm_components(components_dir)
    .into_iter()
    .map(|path| -> std::io::Result<(Vec<u8>, std::path::PathBuf)> {
      return Ok((std::fs::read(&path)?, path));
    })
    .collect::<Result<Vec<_>, _>>()?;

  return components
    .into_iter()
    .map(|(bytes, path)| -> Result<Component, BoxError> {
      let wit_component::DecodedWasm::Component(mut resolve, _world_id) =
        wit_component::decode(&bytes)?
      else {
        return Err("Not a component".into());
      };

      resolve.importize(_world_id, None)?;
      resolve.merge_world_imports_based_on_semver(_world_id)?;

      let packages: Vec<_> = resolve
        .packages
        .iter()
        .map(|p| {
          let package = p.1;

          return Package {
            name: package.name.name.clone(),
            namespace: package.name.namespace.clone(),
            version: package.name.version.as_ref().map(|v| v.to_string()),
            worlds: package
              .worlds
              .iter()
              .map(|(name, _idx)| name.clone())
              .collect(),
            interfaces: package
              .interfaces
              .iter()
              .map(|(name, _idx)| name.clone())
              .collect(),
          };
        })
        .collect();

      return Ok(Component { path, packages });
    })
    .collect();
}

#[derive(Clone, Debug)]
pub enum ComponentReference {
  Path(std::path::PathBuf),
  Url(url::Url),
  Name(String),
}

impl ComponentReference {
  pub fn parse(reference: &str) -> Result<Self, String> {
    if let Ok(url) = url::Url::parse(reference) {
      if url.scheme() != "https" {
        return Err("Only HTTPS supported".into());
      }

      return Ok(ComponentReference::Url(url));
    }

    let path = std::path::PathBuf::from(reference);
    if let Some(ext) = path.extension() {
      match &*ext.to_string_lossy() {
        "wasm" | "zip" => {
          return Ok(ComponentReference::Path(path));
        }
        _ => {}
      }
    }

    if reference
      .chars()
      .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '/')
    {
      return Ok(ComponentReference::Name(reference.into()));
    }

    return Err("Failed to parse component reference".into());
  }
}

impl TryFrom<&str> for ComponentReference {
  type Error = String;

  fn try_from(reference: &str) -> Result<Self, Self::Error> {
    return ComponentReference::parse(reference);
  }
}
