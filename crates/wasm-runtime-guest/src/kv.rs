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

  pub fn get(&self, key: &str) -> Option<Vec<u8>> {
    return self.bucket.get(key).ok().flatten();
  }

  pub fn set(&mut self, key: &str, value: &[u8]) {
    self.bucket.set(key, value).unwrap();
  }

  pub fn delete(&mut self, key: &str) {
    self.bucket.delete(key).unwrap();
  }

  pub fn exists(&mut self, key: &str) -> bool {
    return self.bucket.exists(key).unwrap_or(false);
  }
}
