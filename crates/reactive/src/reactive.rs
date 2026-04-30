use parking_lot::{Mutex, RwLock};
use std::fmt::Debug;
use std::ops::DerefMut;
use std::sync::Arc;

pub struct DeriveInput<'a, T, D> {
  /// Previous value of `this` Reactive. Can be None on first initialization.
  pub prev: Option<&'a Arc<T>>,
  /// Dependent's value.
  pub dep: &'a Arc<D>,
}

type Observer<T> = Box<dyn FnMut(&Arc<T>) + Send + Sync>;

#[derive(Default)]
struct State<T> {
  value: RwLock<Arc<T>>,
  observers: Mutex<Vec<Observer<T>>>,
}

#[derive(Clone, Default)]
pub struct Reactive<T> {
  state: Arc<State<T>>,
}

impl<T> Reactive<T> {
  /// Constructs a new `Reactive<T>`
  pub fn new(value: T) -> Self {
    Self {
      state: Arc::new(State {
        value: RwLock::new(Arc::new(value)),
        observers: Default::default(),
      }),
    }
  }

  // pub fn from(value: Arc<T>) -> Self {
  //   Self {
  //     state: Arc::new(State {
  //       value: ArcSwap::from(value),
  //       observers: Default::default(),
  //     }),
  //   }
  // }

  /// Returns a clone/copy of the value inside the reactive.
  pub fn value(&self) -> T
  where
    T: Clone,
  {
    return (**self.state.value.read()).clone();
  }

  /// Returns a copy of the intenral pointer.
  pub fn ptr(&self) -> Arc<T>
  where
    T: Clone,
  {
    return self.state.value.read().clone();
  }

  /// Perform some action with the reference to the inner value.
  pub fn with_value(&self, f: impl FnOnce(&T)) {
    f(&self.state.value.read());
  }

  /// Derive a new child reactive that changes whenever the parent reactive changes.
  /// (achieved by adding an observer function to the parent reactive behind the scenes)
  ///
  /// TODO: API should use DeriveInput.
  pub fn derive<U: Clone + PartialEq + Send + Sync + 'static>(
    &self,
    f: impl Fn(&T) -> U + Send + Sync + 'static,
  ) -> Reactive<U>
  where
    T: Clone,
  {
    // NOTE: This is racy. Time passes between derived initialization and registration of
    // observer, i.e. updates may get lost, thus the derived value representing a stale value until
    // next update.
    let derived_val = f(&self.state.value.read());
    let derived: Reactive<U> = Reactive::new(derived_val);

    self.add_observer({
      let derived = derived.clone();
      move |value| derived.update(|_| f(value))
    });

    return derived;
  }

  /// Unlike Reactive::derive, doesn't require PartialEq.
  ///
  /// TODO: API should use DeriveInput.
  pub fn derive_unchecked<U>(&self, f: impl (Fn(&T) -> U) + Send + Sync + 'static) -> Reactive<U>
  where
    T: Clone,
    U: Clone + Send + Sync + 'static,
  {
    // NOTE: This is racy. Time passes between derived initialization and registration of
    // observer, i.e. updates may get lost, thus the derived value representing a stale value until
    // next update.
    let derived_val = f(&self.state.value.read());
    let derived: Reactive<U> = Reactive::new(derived_val);

    self.add_observer({
      let derived = derived.clone();
      move |value| {
        let new_value = f(value);
        derived.update_unchecked(move |_| new_value)
      }
    });

    return derived;
  }

  /// Will update the value eventually.
  ///
  /// TODO: API should use DeriveInput.
  pub async fn derive_unchecked_async<U, F>(
    &self,
    f: impl (Fn(DeriveInput<'_, U, T>) -> F) + Send + Sync + 'static,
  ) -> Reactive<U>
  where
    T: Clone + Send + Sync + 'static,
    F: futures_util::Future<Output = U> + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
  {
    // NOTE: This is racy. Time passes between derived initialization and registration of
    // observer, i.e. updates may get lost, thus the derived value representing a stale value until
    // next update.
    let val: Arc<T> = self.state.value.read().clone();
    let derived_val = f(DeriveInput {
      prev: None,
      dep: &val,
    })
    .await;
    let derived: Reactive<U> = Reactive::new(derived_val);

    self.add_observer({
      let derived = derived.clone();
      let f = Arc::new(f);

      move |value: &Arc<T>| {
        println!("OBS TRIGGERED");
        let value = value.clone();
        let derived = derived.clone();
        let f = f.clone();

        derived.update_unchecked_async(move |old: &Arc<U>| {
          println!("UPDATE");
          let old = old.clone();
          return Box::pin(async move {
            return (*f)(DeriveInput {
              prev: Some(&old),
              dep: &value,
            })
            .await;
          });
        });
      }
    });

    return derived;
  }

  /// Adds a new observer to the reactive.
  pub fn add_observer(&self, mut f: impl FnMut(&Arc<T>) + Send + Sync + 'static) {
    return self.state.observers.lock().push(Box::new(move |v| f(v)));
  }

  /// Clears all observers from the reactive.
  pub fn clear_observers(&self) {
    self.state.observers.lock().clear();
  }

  /// Set the value inside the reactive to something new and notify all the observers
  /// by calling the added observer functions in the sequence they were added
  /// (even if the provided value is the same as the current one)
  pub fn set(&self, val: T) {
    self.update_unchecked(move |_| val);
  }

  /// Update the value inside the reactive and notify all the observers
  /// by calling the added observer functions in the sequence they were added
  /// **ONLY** if the value changes after applying the provided function
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(10);
  /// let d = r.derive(|val| val + 5);
  ///
  /// r.update(|_| 20);
  ///
  /// assert_eq!(25, d.value());
  /// ```
  pub fn update(&self, f: impl FnOnce(&T) -> T)
  where
    T: PartialEq,
  {
    let mut lock = self.state.value.upgradable_read();
    let old_val: &T = &lock;
    let new_val = f(old_val);
    if &new_val != old_val {
      lock.with_upgraded(|rw| {
        *rw = Arc::new(new_val);

        for obs in self.state.observers.lock().deref_mut() {
          obs(rw);
        }
      });
    }
  }

  /// Update the value inside the reactive and notify all the observers
  /// by calling the added observer functions in the sequence they were added
  /// without checking if the value is changed after applying the provided function
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(10);
  /// let d = r.derive(|val| val + 5);
  ///
  /// // notifies the observers as usual because value changed from 10 to 20
  /// r.update_unchecked(|_| 20);
  ///
  /// assert_eq!(25, d.value());
  ///
  /// // would still notify the observers even if the value didn't change
  /// r.update_unchecked(|_| 20);
  ///
  /// assert_eq!(25, d.value());
  /// ```
  ///
  /// # Reasons to use
  /// `update_unchecked` doesn't require `PartialEq` trait bounds on `T`
  /// because the old value and the new value (after applying `f`) aren't compared.
  ///
  /// It is also faster than `update` for that reason
  pub fn update_unchecked(&self, f: impl FnOnce(&T) -> T) {
    let mut lock = self.state.value.upgradable_read();
    let new_val = Arc::new(f(&lock));

    lock.with_upgraded(|rw| {
      *rw = new_val;

      for obs in self.state.observers.lock().deref_mut() {
        obs(rw);
      }
    });
  }

  /// Eventually updates the reactive.
  ///
  /// NOTE: We're deliberately holding a lock across await points for consistency but delegate to a
  /// background thread to avoid deadlocks for small runtime worker pools.
  pub fn update_unchecked_async<F>(&self, f: impl (FnOnce(&Arc<T>) -> F) + Send + Sync + 'static)
  where
    T: Send + Sync + 'static,
    F: futures_util::Future<Output = T> + Send + Sync + 'static,
  {
    let state = self.state.clone();
    println!("HERE");

    let h = tokio::runtime::Handle::current();

    #[allow(clippy::await_holding_lock)]
    let _ = h.spawn(async move {
      println!("WTF");
      // WARN: We're holding a lock here across `.await` points.
      let mut lock = state.value.write();
      *lock = Arc::new(f(&lock).await);

      for obs in state.observers.lock().deref_mut() {
        obs(&lock);
      }
    });
  }

  // pub fn update_unchecked_ptr(&self, f: impl FnOnce(&Arc<T>) -> T) {
  //   let val = self.state.value.load();
  //   let new_val = Arc::new(f(&val));
  //   self.state.value.store(new_val.clone());
  //
  //   for obs in self.state.observers.lock().deref_mut() {
  //     obs(&new_val);
  //   }
  // }

  /// Notify all the observers of the current value by calling the
  /// added observer functions in the sequence they were added
  ///
  /// # Examples
  ///
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(String::from("🦀"));
  /// r.add_observer(|val| println!("{}", val));
  /// r.notify();
  /// ```
  pub fn notify(&self) {
    let lock = self.state.value.read();
    for obs in self.state.observers.lock().deref_mut() {
      obs(&lock);
    }
  }
}

impl<T: Debug> Debug for Reactive<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("Reactive")
      .field(&self.state.value.read())
      .finish()
  }
}
