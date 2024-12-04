use arc_swap::{ArcSwap, AsRaw, Guard};
use parking_lot::Mutex;
use std::sync::Arc;

type Listener<T> = Box<dyn Fn(&T) + Sync + Send>;

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

    for callback in self.listeners.lock().iter() {
      callback(&*new);
    }
    return true;
  }

  pub fn listen<F>(&self, callback: F)
  where
    F: 'static + Sync + Send + Fn(&T),
  {
    self.listeners.lock().push(Box::new(callback));
  }

  pub fn store(&self, v: T) {
    let ptr = Arc::new(v);
    self.value.store(ptr.clone());

    for callback in self.listeners.lock().iter() {
      callback(&*ptr);
    }
  }
}

struct ComputedState<T, V> {
  value: ArcSwap<T>,
  f: Box<dyn Sync + Send + Fn(&V) -> T>,
}

pub struct Computed<T, V> {
  state: Arc<ComputedState<T, V>>,
}

impl<T: 'static + Sync + Send, V: 'static> Computed<T, V> {
  pub fn new(notifier: &ValueNotifier<V>, f: impl 'static + Sync + Send + Fn(&V) -> T) -> Self {
    let state = Arc::new(ComputedState {
      value: ArcSwap::<T>::from_pointee(f(&notifier.load())),
      f: Box::new(f),
    });

    let state_ptr = state.clone();
    notifier.listen(move |v| {
      state_ptr.value.store(Arc::new((*state_ptr.f)(v)));
    });

    return Computed { state };
  }

  pub fn load(&self) -> Guard<Arc<T>> {
    return self.state.value.load();
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

    let c = Computed::new(&v, |v| v * 2);
    assert_eq!(**c.load(), 2 * 42);

    v.store(23);
    assert_eq!(**c.load(), 2 * 23);
  }
}
