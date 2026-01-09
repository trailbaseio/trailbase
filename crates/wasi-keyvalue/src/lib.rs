//! # Wasmtime's [wasi-keyvalue] Implementation
//!
//! This crate provides a Wasmtime host implementation of the [wasi-keyvalue]
//! API. With this crate, the runtime can run components that call APIs in
//! [wasi-keyvalue] and provide components with access to key-value storages.
//!
//! Currently supported storage backends:
//! * In-Memory (empty identifier)

#![allow(clippy::needless_return)]
#![deny(missing_docs)]
#![forbid(clippy::unwrap_used)]
#![warn(clippy::await_holding_lock, clippy::inefficient_to_string)]

mod generated {
  wasmtime::component::bindgen!({
      path: "wit",
      world: "wasi:keyvalue/imports",
      imports: { default: trappable },
      with: {
        "wasi:keyvalue/store.bucket": crate::Bucket,
      },
      trappable_error_type: {
        "wasi:keyvalue/store.error" => crate::Error,
      },
  });
}

use self::generated::wasi::keyvalue;

use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use wasmtime::component::{HasData, Resource, ResourceTable, ResourceTableError};

#[doc(hidden)]
pub enum Error {
  NoSuchStore,
  AccessDenied,
  Other(String),
}

impl From<ResourceTableError> for Error {
  fn from(err: ResourceTableError) -> Self {
    Self::Other(err.to_string())
  }
}

type InternalStore = Arc<RwLock<HashMap<String, Vec<u8>>>>;

/// The practical type for the inmemory Store.
#[derive(Clone, Default)]
pub struct Store {
  store: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl Store {
  /// New shared storage for WASI KV implementation.
  pub fn new() -> Self {
    return Store::default();
  }

  /// Insert new value. Returns not-None if value with key already existed.
  pub fn set(&self, key: String, value: Vec<u8>) -> Option<Vec<u8>> {
    return self.store.write().insert(key, value);
  }

  /// Get a store value.
  pub fn get(&self, key: &str) -> Option<Vec<u8>> {
    return self.store.read().get(key).cloned();
  }
}

#[doc(hidden)]
pub struct Bucket {
  in_memory_data: InternalStore,
}

/// Capture the state necessary for use in the `wasi-keyvalue` API implementation.
pub struct WasiKeyValueCtx {
  in_memory_data: InternalStore,
}

impl WasiKeyValueCtx {
  /// Inject shared data.
  pub fn new(data: Store) -> Self {
    return Self {
      in_memory_data: data.store,
    };
  }
}

/// A wrapper capturing the needed internal `wasi-keyvalue` state.
pub struct WasiKeyValue<'a> {
  ctx: &'a WasiKeyValueCtx,
  table: &'a mut ResourceTable,
}

impl<'a> WasiKeyValue<'a> {
  /// Create a new view into the `wasi-keyvalue` state.
  pub fn new(ctx: &'a WasiKeyValueCtx, table: &'a mut ResourceTable) -> Self {
    Self { ctx, table }
  }
}

impl keyvalue::store::Host for WasiKeyValue<'_> {
  fn open(&mut self, identifier: String) -> Result<Resource<Bucket>, Error> {
    match identifier.as_str() {
      "" => Ok(self.table.push(Bucket {
        in_memory_data: self.ctx.in_memory_data.clone(),
      })?),
      _ => Err(Error::NoSuchStore),
    }
  }

  fn convert_error(&mut self, err: Error) -> Result<keyvalue::store::Error> {
    match err {
      Error::NoSuchStore => Ok(keyvalue::store::Error::NoSuchStore),
      Error::AccessDenied => Ok(keyvalue::store::Error::AccessDenied),
      Error::Other(e) => Ok(keyvalue::store::Error::Other(e)),
    }
  }
}

impl keyvalue::store::HostBucket for WasiKeyValue<'_> {
  fn get(&mut self, bucket: Resource<Bucket>, key: String) -> Result<Option<Vec<u8>>, Error> {
    let bucket = self.table.get_mut(&bucket)?;
    Ok(bucket.in_memory_data.read().get(&key).cloned())
  }

  fn set(&mut self, bucket: Resource<Bucket>, key: String, value: Vec<u8>) -> Result<(), Error> {
    let bucket = self.table.get_mut(&bucket)?;
    bucket.in_memory_data.write().insert(key, value);
    Ok(())
  }

  fn delete(&mut self, bucket: Resource<Bucket>, key: String) -> Result<(), Error> {
    let bucket = self.table.get_mut(&bucket)?;
    bucket.in_memory_data.write().remove(&key);
    Ok(())
  }

  fn exists(&mut self, bucket: Resource<Bucket>, key: String) -> Result<bool, Error> {
    let bucket = self.table.get_mut(&bucket)?;
    Ok(bucket.in_memory_data.read().contains_key(&key))
  }

  fn list_keys(
    &mut self,
    bucket: Resource<Bucket>,
    cursor: Option<u64>,
  ) -> Result<keyvalue::store::KeyResponse, Error> {
    let bucket = self.table.get_mut(&bucket)?;
    let keys: Vec<String> = bucket.in_memory_data.read().keys().cloned().collect();
    let cursor = cursor.unwrap_or(0) as usize;
    let keys_slice = &keys[cursor..];
    Ok(keyvalue::store::KeyResponse {
      keys: keys_slice.to_vec(),
      cursor: None,
    })
  }

  fn drop(&mut self, bucket: Resource<Bucket>) -> Result<()> {
    self.table.delete(bucket)?;
    Ok(())
  }
}

impl keyvalue::atomics::Host for WasiKeyValue<'_> {
  fn increment(&mut self, bucket: Resource<Bucket>, key: String, delta: u64) -> Result<u64, Error> {
    let bucket = self.table.get_mut(&bucket)?;
    let mut data = bucket.in_memory_data.write();
    let value = data.entry(key.clone()).or_insert(b"0".to_vec());

    let current_value = String::from_utf8(value.clone())
      .map_err(|e| Error::Other(e.to_string()))?
      .parse::<u64>()
      .map_err(|e| Error::Other(e.to_string()))?;
    let new_value = current_value + delta;

    *value = new_value.to_string().into_bytes();
    Ok(new_value)
  }
}

impl keyvalue::batch::Host for WasiKeyValue<'_> {
  fn get_many(
    &mut self,
    bucket: Resource<Bucket>,
    keys: Vec<String>,
  ) -> Result<Vec<Option<(String, Vec<u8>)>>, Error> {
    let bucket = self.table.get_mut(&bucket)?;
    let lock = bucket.in_memory_data.read();
    Ok(
      keys
        .into_iter()
        .map(|key| lock.get(&key).map(|value| (key.clone(), value.clone())))
        .collect(),
    )
  }

  fn set_many(
    &mut self,
    bucket: Resource<Bucket>,
    key_values: Vec<(String, Vec<u8>)>,
  ) -> Result<(), Error> {
    let bucket = self.table.get_mut(&bucket)?;
    let mut lock = bucket.in_memory_data.write();
    for (key, value) in key_values {
      lock.insert(key, value);
    }
    Ok(())
  }

  fn delete_many(&mut self, bucket: Resource<Bucket>, keys: Vec<String>) -> Result<(), Error> {
    let bucket = self.table.get_mut(&bucket)?;
    let mut lock = bucket.in_memory_data.write();
    for key in keys {
      lock.remove(&key);
    }
    Ok(())
  }
}

/// Add all the `wasi-keyvalue` world's interfaces to a [`wasmtime::component::Linker`].
pub fn add_to_linker<T: Send + 'static>(
  l: &mut wasmtime::component::Linker<T>,
  f: fn(&mut T) -> WasiKeyValue<'_>,
) -> Result<()> {
  keyvalue::store::add_to_linker::<_, HasWasiKeyValue>(l, f)?;
  keyvalue::atomics::add_to_linker::<_, HasWasiKeyValue>(l, f)?;
  keyvalue::batch::add_to_linker::<_, HasWasiKeyValue>(l, f)?;
  Ok(())
}

struct HasWasiKeyValue;

impl HasData for HasWasiKeyValue {
  type Data<'a> = WasiKeyValue<'a>;
}
