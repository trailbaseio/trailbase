use std::sync::Arc;

pub enum CowArc<'a, T: ?Sized + 'static> {
  Borrowed(&'a T),
  Owned(Arc<T>),
}

impl<'a, T: ?Sized + 'static> std::ops::Deref for CowArc<'a, T> {
  type Target = T;

  fn deref(&self) -> &T {
    match self {
      CowArc::Borrowed(b) => b,
      CowArc::Owned(o) => o,
    }
  }
}

impl<T: std::fmt::Debug + ?Sized + 'static> std::fmt::Debug for CowArc<'_, T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      CowArc::Borrowed(b) => std::fmt::Debug::fmt(b, f),
      CowArc::Owned(o) => std::fmt::Debug::fmt(o, f),
    }
  }
}
