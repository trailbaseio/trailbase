pub use crate::wit::wasi::keyvalue::store::Error;
use crate::wit::wasi::keyvalue::store::{Bucket, open as wit_open};

pub fn open() -> Result<Bucket, String> {
  return wit_open("").map_err(|err| err.to_string());
}

pub struct Store {
  bucket: Bucket,
}

impl Store {
  pub fn open() -> Result<Self, String> {
    return Ok(Self { bucket: open()? });
  }

  pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
    return self.bucket.get(key);
  }

  pub fn set(&mut self, key: &str, value: &[u8]) -> Result<(), Error> {
    return self.bucket.set(key, value);
  }

  pub fn delete(&mut self, key: &str) -> Result<(), Error> {
    return self.bucket.delete(key);
  }

  pub fn exists(&mut self, key: &str) -> Result<bool, Error> {
    return self.bucket.exists(key);
  }
}
