use std::ops::{Deref, DerefMut};

use crate::sqlite::executor::ConnectionVec;

#[derive(thiserror::Error, Debug)]
pub enum LockError {
  #[error("Timeout")]
  Timeout,
  #[error("NotSupported")]
  NotSupported,
}

pub struct LockGuard<'a>(parking_lot::RwLockWriteGuard<'a, ConnectionVec>);

impl<'a> LockGuard<'a> {
  pub(super) fn new(guard: parking_lot::RwLockWriteGuard<'a, ConnectionVec>) -> Self {
    return Self(guard);
  }
}

impl Deref for LockGuard<'_> {
  type Target = rusqlite::Connection;

  #[inline]
  fn deref(&self) -> &Self::Target {
    return &self.0.deref().0[0];
  }
}

impl DerefMut for LockGuard<'_> {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    return &mut self.0.deref_mut().0[0];
  }
}

pub struct ArcLockGuard {
  pub(super) guard: parking_lot::ArcRwLockWriteGuard<parking_lot::RawRwLock, ConnectionVec>,
}

impl Deref for ArcLockGuard {
  type Target = rusqlite::Connection;

  #[inline]
  fn deref(&self) -> &Self::Target {
    return &self.guard.deref().0[0];
  }
}

impl DerefMut for ArcLockGuard {
  #[inline]
  fn deref_mut(&mut self) -> &mut Self::Target {
    return &mut self.guard.deref_mut().0[0];
  }
}
