use futures_util::FutureExt;
use futures_util::future::{BoxFuture, Shared};
use parking_lot::{Mutex, RwLock};
use std::fmt::Debug;
use std::sync::Arc;

type Observer<T> = Box<dyn FnMut(&Arc<T>) + Send + Sync>;

struct State<T> {
  future: Mutex<Shared<BoxFuture<'static, Arc<T>>>>,
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
// NOTE: T has to be sync to allow concurrent reads via RwLock.
impl<T: Send + Sync + 'static> AsyncReactive<T> {
  pub async fn new<F, Fut>(f: F) -> Self
  where
    F: (FnOnce() -> Fut) + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
  {
    let first_value = Arc::new(f().await);

    return Self {
      state: Arc::new(State {
        future: Mutex::new(
          futures_util::future::ready(first_value.clone())
            .boxed()
            .shared(),
        ),
        snapshot: RwLock::new(first_value),
        observers: Default::default(),
      }),
    };
  }

  /// Awaits the internal future.
  pub async fn ptr(&self) -> Arc<T> {
    let fut = self.state.future.lock().clone();
    return fut.await.clone();
  }

  pub fn snapshot(&self) -> Arc<T> {
    return self.state.snapshot.read().clone();
  }

  /// Updates the reactive's value based on the given function.
  ///
  /// The update semantics are quite complex due async reactive having both a future and a
  /// snapshot state. This implementation here synchronously updates the future state but the
  /// snapshot is only updated after the computation of `f` completes. However, the computation
  /// is only triggered if update_unchecked is awaited or `self.ptr()` is polled.
  pub fn update_unchecked<F, Fut>(&self, f: F) -> BoxFuture<'static, ()>
  where
    F: (FnOnce(Arc<T>) -> Fut) + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
  {
    let mut lock = self.state.future.lock();

    let new_fut = Box::pin({
      let state = self.state.clone();
      let old_fut = lock.clone();

      async move {
        let new_value = Arc::new(f(old_fut.await).await);

        *state.snapshot.write() = new_value.clone();

        for obs in &mut *state.observers.lock() {
          obs(&new_value);
        }

        return new_value;
      }
    })
    .boxed()
    .shared();

    *lock = new_fut.clone();

    return Box::pin(async move {
      new_fut.await;
    });
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
    let r = AsyncReactive::new(async || 1).await;

    assert_eq!(1, *r.ptr().await);
    assert_eq!(1, *r.snapshot());

    let fut = r.update_unchecked(|old| {
      return Box::pin(async move { *old + 1 });
    });
    assert_eq!(1, *r.snapshot());
    assert_eq!(2, *r.ptr().await);
    assert_eq!(2, *r.snapshot());

    fut.await;

    assert_eq!(2, *r.ptr().await);
    assert_eq!(2, *r.ptr().await);
    assert_eq!(2, *r.snapshot());
  }
}
