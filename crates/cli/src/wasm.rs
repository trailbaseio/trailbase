use std::{
  collections::HashMap,
  io::{Read, Seek},
};
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

pub fn find_component(name: &str) -> Result<ComponentDefinition, BoxError> {
  return repo()
    .get(name)
    .cloned()
    .ok_or_else(|| "component not found".into());
}

pub async fn install_wasm_component(
  data_dir: &DataDir,
  path: impl AsRef<std::path::Path>,
  mut reader: impl Read + Seek,
) -> Result<(), BoxError> {
  let path = path.as_ref();
  let wasm_dir = data_dir.root().join("wasm");

  match path
    .extension()
    .map(|p| p.to_string_lossy().to_string())
    .as_deref()
  {
    Some("zip") => {
      let mut archive = zip::ZipArchive::new(reader)?;

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

          println!("Added: {component_file_path:?}");
        }
      }
    }
    Some("wasm") => {
      let Some(filename) = path.file_name().and_then(|e| e.to_str()) else {
        return Err(format!("Invalid filename: {path:?}").into());
      };

      let component_file_path = wasm_dir.join(filename);
      let mut component_file = std::fs::File::create(&component_file_path)?;
      std::io::copy(&mut reader, &mut component_file)?;

      println!("Added: {component_file_path:?}");
    }
    _ => {
      return Err("unexpected format".into());
    }
  }

  return Ok(());
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
