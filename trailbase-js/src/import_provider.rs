use rustyscript::deno_core::{
  ModuleSpecifier, RequestedModuleType, ResolutionKind,
  anyhow::{Error, anyhow},
};
use rustyscript::module_loader::ImportProvider as RustyScriptImportProvider;

use crate::util::cow_to_string;

#[derive(Default)]
pub struct ImportProvider;

impl RustyScriptImportProvider for ImportProvider {
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
          crate::JsRuntimeAssets::get("index.js")
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
