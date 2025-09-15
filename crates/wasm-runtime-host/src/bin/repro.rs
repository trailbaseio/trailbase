use std::path::PathBuf;
use trailbase_sqlite::Connection;
use trailbase_wasm_runtime_host::{KvStore, Runtime};

#[tokio::main]
async fn main() {
  let args: Vec<String> = std::env::args().collect();

  let path: PathBuf = args
    .get(1)
    .cloned()
    .unwrap_or("guests/dotnet/bin/Release/net10.0/wasi-wasm/publish/Guest.wasm".to_string())
    .into();

  println!("Initializing component: {path:?}");

  let runtime = Runtime::new(
    2,
    path,
    Connection::open_in_memory().unwrap(),
    KvStore::new(),
    None,
  )
  .unwrap();

  let result = runtime.call(async |rt| rt.call_init().await).await;

  println!("{result:?}");
}
