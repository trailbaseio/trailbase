use arc_swap::{ArcSwap, AsRaw};
use parking_lot::Mutex;
use std::sync::Arc;

pub use arc_swap::Guard;

type Listener<T> = Box<dyn Fn(&T) -> bool + Sync + Send>;

pub struct ValueNotifier<T> {
  value: ArcSwap<T>,
  listeners: Mutex<Vec<Listener<T>>>,
}

impl<T> ValueNotifier<T> {
  pub fn new(v: T) -> Self {
    ValueNotifier {
      value: ArcSwap::from_pointee(v),
      listeners: Mutex::new(Vec::new()),
    }
  }

  pub fn load(&self) -> Guard<Arc<T>> {
    return self.value.load();
  }

  #[allow(unused)]
  pub fn load_full(&self) -> Arc<T> {
    return self.value.load_full();
  }

  // Returns true in case of successful swap.
  pub fn compare_and_swap<C>(&self, current: C, new: Arc<T>) -> bool
  where
    C: AsRaw<T>,
  {
    // compare_and_swap returns the previous value no matter if the swap happened or not,
    // i.e. if the returned value is equal to old_config a.k.a. `current`, the swap happened.
    // let old: Arc<T> = self.value.load_full();
    let current_ptr = current.as_raw();
    let prev = self.value.compare_and_swap(current, new.clone());

    if current_ptr != prev.as_raw() {
      return false;
    }

    self.notify(&*new);
    return true;
  }

  pub fn store(&self, v: T) {
    let ptr = Arc::new(v);
    self.value.store(ptr.clone());
    self.notify(&*ptr);
  }

  fn notify(&self, value: &T) {
    let mut lock = self.listeners.lock();
    lock.retain(|callback| callback(value));
  }

  fn listen(&self, callback: Listener<T>) {
    self.listeners.lock().push(callback);
  }
}

#[derive(Clone)]
pub struct Computed<T> {
  value: Arc<ArcSwap<T>>,
}

impl<T: Sync + Send + 'static> Computed<T> {
  pub fn new<V>(notifier: &ValueNotifier<V>, f: impl Fn(&V) -> T + Sync + Send + 'static) -> Self {
    let value = Arc::new(ArcSwap::<T>::from_pointee(f(&notifier.load())));

    let weak = Arc::downgrade(&value);
    notifier.listen(Box::new(move |v| {
      if let Some(arc_swap) = weak.upgrade() {
        arc_swap.store(Arc::new(f(v)));
        return true;
      }

      return false;
    }));

    return Self { value };
  }

  #[inline]
  pub fn load(&self) -> Guard<Arc<T>> {
    return self.value.load();
  }

  #[inline]
  pub fn load_full(&self) -> Arc<T> {
    return self.value.load_full();
  }
}

#[cfg(test)]
mod test {
  use super::*;

  #[test]
  fn test_value_notifier() {
    let v = ValueNotifier::new(42);
    assert_eq!(**v.load(), 42);
    v.store(23);
    assert_eq!(**v.load(), 23);
  }

  #[test]
  fn test_computed() {
    let v = ValueNotifier::new(42);

    {
      let c = Computed::new(&v, |v| v * 2);
      assert_eq!(**c.load(), 2 * 42);

      v.store(23);
      assert_eq!(**c.load(), 2 * 23);
    }

    v.store(5);
    assert_eq!(0, v.listeners.lock().len());
  }
}
