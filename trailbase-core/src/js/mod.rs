use rustyscript::deno_core::{
  anyhow::Error, ModuleSource, ModuleSourceCode, ModuleSpecifier, RequestedModuleType,
  ResolutionKind,
};
use rustyscript::module_loader::ImportProvider;
use std::collections::HashMap;

#[derive(Default)]
pub struct MemoryCache {
  cache: HashMap<String, String>,
}

impl MemoryCache {
  /// Set a module in the cache
  pub fn set(&mut self, specifier: &str, source: String) {
    self.cache.insert(specifier.to_string(), source);
  }

  /// Get a module from the cache
  pub fn get(&self, specifier: &ModuleSpecifier) -> Option<String> {
    self.cache.get(specifier.as_str()).cloned()
  }

  pub fn has(&self, specifier: &ModuleSpecifier) -> bool {
    self.cache.contains_key(specifier.as_str())
  }
}

impl ImportProvider for MemoryCache {
  fn resolve(
    &mut self,
    specifier: &ModuleSpecifier,
    _referrer: &str,
    _kind: ResolutionKind,
  ) -> Option<Result<ModuleSpecifier, Error>> {
    // println!("resolve: {specifier:?}");
    // Tell the loader to allow the import if the module is in the cache
    self.get(specifier).map(|_| Ok(specifier.clone()))
  }

  fn import(
    &mut self,
    specifier: &ModuleSpecifier,
    _referrer: Option<&ModuleSpecifier>,
    _is_dyn_import: bool,
    _requested_module_type: RequestedModuleType,
  ) -> Option<Result<String, Error>> {
    // println!("import : {specifier:?}");
    // Return the source code if the module is in the cache
    self.get(specifier).map(Ok)
  }

  fn post_process(
    &mut self,
    specifier: &ModuleSpecifier,
    source: ModuleSource,
  ) -> Result<ModuleSource, Error> {
    // println!("post_process: {specifier:?}");
    // Cache the source code
    if !self.has(specifier) {
      match &source.code {
        ModuleSourceCode::String(s) => {
          self.set(specifier.as_str(), s.to_string());
        }
        ModuleSourceCode::Bytes(_) => {}
      }
    }
    Ok(source)
  }
}
