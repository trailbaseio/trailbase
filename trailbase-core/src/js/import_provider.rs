use rustyscript::deno_core::{
  anyhow::{anyhow, Error},
  ModuleSpecifier, RequestedModuleType, ResolutionKind,
};
use rustyscript::module_loader::ImportProvider;

use crate::util::cow_to_string;

#[derive(Default)]
pub(crate) struct ImportProviderImpl;

impl ImportProvider for ImportProviderImpl {
  fn resolve(
    &mut self,
    specifier: &ModuleSpecifier,
    _referrer: &str,
    _kind: ResolutionKind,
  ) -> Option<Result<ModuleSpecifier, Error>> {
    log::trace!("resolve: {specifier:?}");

    // Specifier is just a URL.
    match specifier.scheme() {
      "file" | "trailbase" => {
        return Some(Ok(specifier.clone()));
      }
      scheme => {
        return Some(Err(anyhow!("Unsupported schema: '{scheme}'")));
      }
    };
  }

  fn import(
    &mut self,
    specifier: &ModuleSpecifier,
    _referrer: Option<&ModuleSpecifier>,
    _is_dyn_import: bool,
    _requested_module_type: RequestedModuleType,
  ) -> Option<Result<String, Error>> {
    log::trace!("import: {specifier:?}");

    match specifier.scheme() {
      "trailbase" => {
        return Some(Ok(cow_to_string(
          trailbase_assets::JsRuntimeAssets::get("index.js")
            .expect("Failed to read rt/index.js")
            .data,
        )));
      }
      _ => {
        return None;
      }
    }
  }
}
