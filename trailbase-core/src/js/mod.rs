use rustyscript::deno_core::{
  anyhow::Error, ModuleSource, ModuleSourceCode, ModuleSpecifier, RequestedModuleType,
  ResolutionKind,
};
use rustyscript::module_loader::ImportProvider;
use rustyscript::Runtime;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Default)]
struct MemoryCache {
  cache: HashMap<String, String>,
}

impl MemoryCache {
  /// Set a module in the cache
  fn set(&mut self, specifier: &str, source: String) {
    self.cache.insert(specifier.to_string(), source);
  }

  /// Get a module from the cache
  fn get(&self, specifier: &ModuleSpecifier) -> Option<String> {
    self.cache.get(specifier.as_str()).cloned()
  }

  fn has(&self, specifier: &ModuleSpecifier) -> bool {
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

enum Message {
  Close,
  Run(Box<dyn (FnOnce(&mut Runtime)) + Send + Sync>),
}

struct RuntimeWrapper {
  sender: crossbeam_channel::Sender<Message>,
  handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for RuntimeWrapper {
  fn drop(&mut self) {
    if let Some(handle) = self.handle.take() {
      self.sender.send(Message::Close).unwrap();
      handle.join().unwrap();
    }
  }
}

fn init_runtime() -> Result<Runtime, AnyError> {
  let mut cache = MemoryCache::default();

  cache.set(
    "trailbase:main",
    r#"
      export const fun = async () => {
        console.log('fun0.log');
      };
    "#
    .to_string(),
  );

  let runtime = rustyscript::Runtime::new(rustyscript::RuntimeOptions {
    import_provider: Some(Box::new(cache)),
    schema_whlist: HashSet::from(["trailbase".to_string()]),
    ..Default::default()
  })?;

  return Ok(runtime);
}

impl RuntimeWrapper {
  fn new() -> Self {
    let (sender, receiver) = crossbeam_channel::unbounded::<Message>();

    let handle = std::thread::spawn(move || {
      let mut runtime = init_runtime().unwrap();

      #[allow(clippy::never_loop)]
      while let Ok(message) = receiver.recv() {
        match message {
          Message::Close => break,
          Message::Run(f) => {
            f(&mut runtime);
          }
        }
      }
    });

    let _ = sender.send(Message::Run(Box::new(
      |runtime: &mut rustyscript::Runtime| {
        let module = rustyscript::Module::new(
          "trailbase:main",
          r#"
            import { fun } from "trailbase:main";
            export const fun0 = fun;
          "#,
        );
        let handle = runtime.load_module(&module).unwrap();
        let _foo: rustyscript::js_value::Promise<String> = runtime
          .call_function_immediate(Some(&handle), "fun0", rustyscript::json_args!())
          .unwrap();
      },
    )));

    return RuntimeWrapper {
      sender,
      handle: Some(handle),
    };
  }
}

// NOTE: Repeated runtime initialization, e.g. in a multi-threaded context, leads to segfaults.
// rustyscript::init_platform is supposed to help with this but we haven't found a way to
// make it work. Thus, we're making the V8 VM a singleton (like Dart's).
static RUNTIME: LazyLock<RuntimeWrapper> = LazyLock::new(RuntimeWrapper::new);

pub(crate) struct RuntimeHandle;

impl RuntimeHandle {
  pub(crate) fn new() -> Self {
    return Self {};
  }

  #[allow(unused)]
  pub(crate) async fn apply<T>(
    &self,
    f: impl (FnOnce(&mut rustyscript::Runtime) -> T) + Send + Sync + 'static,
  ) -> Result<Box<T>, AnyError>
  where
    T: Send + Sync + 'static,
  {
    let (sender, mut receiver) = tokio::sync::oneshot::channel::<Box<T>>();

    RUNTIME.sender.send(Message::Run(Box::new(move |rt| {
      if let Err(_err) = sender.send(Box::new(f(rt))) {
        log::warn!("Failed to send");
      }
    })))?;

    return Ok(receiver.await?);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use rustyscript::{json_args, Module};

  #[tokio::test]
  async fn test_runtime_apply() {
    let handle = RuntimeHandle::new();
    let number = handle
      .apply::<i64>(|_runtime| {
        return 42;
      })
      .await
      .unwrap();

    assert_eq!(42, *number);
  }

  #[tokio::test]
  async fn test_runtime_javascript() {
    let handle = RuntimeHandle::new();
    let result = handle
      .apply::<String>(|runtime| {
        let context = runtime
          .load_module(&Module::new(
            "module.js",
            r#"
              export function test_fun() {
                return "foo";
              }
            "#,
          ))
          .map_err(|err| {
            log::error!("Failed to load module: {err}");
            return err;
          })
          .unwrap();

        return runtime
          .call_function(Some(&context), "test_fun", json_args!())
          .map_err(|err| {
            log::error!("Failed to load call fun: {err}");
            return err;
          })
          .unwrap();
      })
      .await
      .unwrap();

    assert_eq!("foo", *result);
  }
}
