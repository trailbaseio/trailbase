use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek};
use trailbase::DataDir;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ComponentDefinition {
  pub url_template: String,
  pub wasm_filenames: Vec<String>,
}

pub fn repo() -> HashMap<String, ComponentDefinition> {
  return HashMap::from([
        ("trailbase/auth_ui".to_string(), ComponentDefinition {
            url_template: "https://github.com/trailbaseio/trailbase/releases/download/{{ release }}/trailbase_{{ release }}_wasm_auth_ui.zip".to_string(),
            wasm_filenames: vec!["auth_ui_component.wasm".to_string()],
        })
    ]);
}

pub fn find_component(name: &str) -> Option<ComponentDefinition> {
  return repo().get(name).cloned();
}

pub fn find_component_by_filename(filename: &str) -> Option<ComponentDefinition> {
  return repo().into_values().find(|component_def| {
    return component_def
      .wasm_filenames
      .iter()
      .any(|f| f.as_str() == filename);
  });
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

pub async fn install_wasm_component(
  data_dir: &DataDir,
  path: impl AsRef<std::path::Path>,
  mut reader: impl Read + Seek,
) -> Result<Vec<std::path::PathBuf>, BoxError> {
  let path = path.as_ref();
  let wasm_dir = data_dir.root().join("wasm");

  if !fs::exists(&wasm_dir)? {
    fs::create_dir_all(&wasm_dir)?;
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
          let component_file_path = wasm_dir.join(filename);
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

      let component_file_path = wasm_dir.join(filename);
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

pub fn list_installed_wasm_components(data_dir: &DataDir) -> Result<Vec<Component>, BoxError> {
  let components_path = data_dir.root().join("wasm");

  let components: Vec<(Vec<u8>, std::path::PathBuf)> =
    trailbase_wasm_runtime_host::load_wasm_components(
      components_path,
      |path: std::path::PathBuf| -> std::io::Result<(Vec<u8>, std::path::PathBuf)> {
        return Ok((std::fs::read(&path)?, path));
      },
    )?;

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
