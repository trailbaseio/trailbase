use std::ops::{Deref, DerefMut};

use crate::sqlite::executor::ConnectionVec;

#[derive(thiserror::Error, Debug)]
pub enum LockError {
  #[error("Timeout")]
  Timeout,
  #[error("NotSupported")]
  NotSupported,
}

pub struct LockGuard<'a> {
  pub(super) guard: parking_lot::RwLockWriteGuard<'a, ConnectionVec>,
}

impl Deref for LockGuard<'_> {
  type Target = rusqlite::Connection;
  #[inline]
  fn deref(&self) -> &rusqlite::Connection {
    return &self.guard.deref().0[0];
  }
}

impl DerefMut for LockGuard<'_> {
  #[inline]
  fn deref_mut(&mut self) -> &mut rusqlite::Connection {
    return &mut self.guard.deref_mut().0[0];
  }
}

pub struct ArcLockGuard {
  pub(super) guard: parking_lot::ArcRwLockWriteGuard<parking_lot::RawRwLock, ConnectionVec>,
}

impl Deref for ArcLockGuard {
  type Target = rusqlite::Connection;
  #[inline]
  fn deref(&self) -> &rusqlite::Connection {
    return &self.guard.deref().0[0];
  }
}

impl DerefMut for ArcLockGuard {
  #[inline]
  fn deref_mut(&mut self) -> &mut rusqlite::Connection {
    return &mut self.guard.deref_mut().0[0];
  }
}
