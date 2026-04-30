use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use parking_lot::{Mutex, RwLock};
use std::fmt::Debug;
use std::sync::Arc;

type Observer<T> = Box<dyn FnMut(&Arc<T>) + Send + Sync>;

struct State<T> {
  future: RwLock<Shared<BoxFuture<'static, Arc<T>>>>,
  snapshot: RwLock<Arc<T>>,
  observers: Mutex<Vec<Observer<T>>>,
}

pub struct AsyncReactive<T> {
  state: Arc<State<T>>,
}

impl<T> Clone for AsyncReactive<T> {
  fn clone(&self) -> Self {
    return Self {
      state: self.state.clone(),
    };
  }
}

// NOTE: This is currently a best effort implementation replicating of Reactive's API but async.
impl<T: Default + Send + Sync + 'static> AsyncReactive<T> {
  /// Constructs a new `Reactive<T>`
  pub fn new<F, Fut>(f: F) -> Self
  where
    F: (FnOnce() -> Fut) + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
  {
    let state = Arc::new(State {
      future: RwLock::new(
        futures_util::future::ready(Arc::new(T::default()))
          .boxed()
          .shared(),
      ),
      snapshot: RwLock::new(Arc::default()),
      observers: Default::default(),
    });

    let h = tokio::runtime::Handle::current();
    let fut = {
      let state = state.clone();
      h.spawn(async move {
        return Arc::new(f().await);
      })
      .then(async move |maybe| {
        let r = maybe.expect("spawn failed");

        *state.snapshot.write() = r.clone();

        return r;
      })
    };

    *state.future.write() = fut.boxed().shared();

    Self { state }
  }

  /// Awaits the internal future.
  pub async fn ptr(&self) -> Arc<T> {
    let fut = self.state.future.read().clone();
    return fut.await.clone();
  }

  pub fn snapshot(&self) -> Arc<T> {
    return self.state.snapshot.read().clone();
  }

  pub fn update_unchecked<F, Fut>(&self, f: F)
  where
    F: (FnOnce(Arc<T>) -> Fut) + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
  {
    let h = tokio::runtime::Handle::current();
    let state = self.state.clone();
    let mut lock = self.state.future.upgradable_read();
    let old_fut = lock.clone();

    let fut = h
      .spawn(async move {
        let old_value = old_fut.await;
        return Arc::new(f(old_value).await);
      })
      .then(async move |maybe| {
        let r = maybe.expect("spawn failed");
        *state.snapshot.write() = r.clone();
        for obs in &mut *state.observers.lock() {
          obs(&r);
        }
        return r;
      });

    lock.with_upgraded(|rw| {
      *rw = fut.boxed().shared();
    })
  }

  /// Adds a new observer to the reactive.
  pub fn add_observer(&self, mut f: impl FnMut(&Arc<T>) + Send + Sync + 'static) {
    return self.state.observers.lock().push(Box::new(move |v| f(v)));
  }
}

impl<T: Debug> Debug for AsyncReactive<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("AsyncReactive")
      // .field(&self.state.value.read())
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn async_reactive_test() {
    let r = AsyncReactive::new(async || 1);
    assert_eq!(0, *r.snapshot());

    assert_eq!(1, *r.ptr().await);
    assert_eq!(1, *r.snapshot());

    r.update_unchecked(|old| {
      return Box::pin(async move { *old + 1 });
    });

    assert_eq!(2, *r.ptr().await);
    assert_eq!(2, *r.snapshot());
  }
}
