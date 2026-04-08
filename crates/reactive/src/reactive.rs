use parking_lot::{Mutex, RwLock};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

type Observer<T> = dyn FnMut(&T);

#[derive(Default)]
struct State<T> {
  value: RwLock<T>,
  observers: Mutex<Vec<Box<Observer<T>>>>,
}

#[derive(Clone, Default)]
pub struct Reactive<T> {
  state: Arc<State<T>>,
}

impl<T> Reactive<T> {
  /// Constructs a new `Reactive<T>`
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new("🦀");
  /// ```
  pub fn new(value: T) -> Self {
    Self {
      state: Arc::new(State {
        value: RwLock::new(value),
        observers: Default::default(),
      }),
    }
  }

  /// Returns a clone/copy of the value inside the reactive
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(String::from("🦀"));
  /// assert_eq!("🦀", r.value());
  /// ```
  pub fn value(&self) -> T
  where
    T: Clone,
  {
    return self.state.value.read().clone();
  }

  /// Perform some action with the reference to the inner value.
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(String::from("🦀"));
  /// r.with_value(|s| println!("{}", s));
  /// ```
  pub fn with_value(&self, f: impl FnOnce(&T)) {
    f(self.state.value.read().deref());
  }

  /// derive a new child reactive that changes whenever the parent reactive changes.
  /// (achieved by adding an observer function to the parent reactive behind the scenes)
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(10);
  /// let d = r.derive(|val| val + 5);
  ///
  /// assert_eq!(15, d.value());
  /// ```
  pub fn derive<U: Clone + PartialEq + Send + 'static>(
    &self,
    f: impl Fn(&T) -> U + Send + 'static,
  ) -> Reactive<U>
  where
    T: Clone,
  {
    let derived_val = f(self.state.value.read().deref());
    let derived: Reactive<U> = Reactive::new(derived_val);

    self.add_observer({
      let derived = derived.clone();
      move |value| derived.update(|_| f(value))
    });

    return derived;
  }

  // Unlike Reactive::derive, doesn't require PartialEq.
  pub fn derive_unchecked<U: Clone + Send + 'static>(
    &self,
    f: impl Fn(&T) -> U + Send + 'static,
  ) -> Reactive<U>
  where
    T: Clone,
  {
    let derived_val = f(self.state.value.read().deref());
    let derived: Reactive<U> = Reactive::new(derived_val);

    self.add_observer({
      let derived = derived.clone();
      move |value| derived.update_unchecked(|_| f(value))
    });

    return derived;
  }

  /// Adds a new observer to the reactive.
  /// the observer functions are called whenever the value inside the Reactive is updated
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(String::from("🦀"));
  /// r.add_observer(|val| println!("{}", val));
  /// ```
  pub fn add_observer(&self, f: impl FnMut(&T) + 'static) {
    return self.state.observers.lock().push(Box::new(f));
  }

  /// Clears all observers from the reactive.
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(10);
  /// let d = r.derive(|val| val + 1);
  ///
  /// r.clear_observers();
  /// r.update(|n| n * 2);
  ///
  /// assert_eq!(20, r.value());
  /// // value of `d` didn't change because `r` cleared its observers
  /// assert_eq!(11, d.value());
  /// ```
  pub fn clear_observers(&self) {
    self.state.observers.lock().clear();
  }

  /// Set the value inside the reactive to something new and notify all the observers
  /// by calling the added observer functions in the sequence they were added
  /// (even if the provided value is the same as the current one)
  ///
  /// # Examples
  /// ```
  /// use trailbase_reactive::Reactive;
  ///
  /// let r = Reactive::new(10);
  /// let d = r.derive(|val| val + 5);
  ///
  /// r.set(20);
  ///
  /// assert_eq!(25, d.value());
  /// ```
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
    let mut guard = self.state.value.write();
    let val = guard.deref_mut();
    let new_val = f(val);
    if &new_val != val {
      *val = new_val;

      for obs in self.state.observers.lock().deref_mut() {
        obs(val);
      }
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
    let mut guard = self.state.value.write();
    let val = guard.deref_mut();
    *val = f(val);

    for obs in self.state.observers.lock().deref_mut() {
      obs(val);
    }
  }

  /// Updates the value inside inplace without creating a new clone/copy and notify
  /// all the observers by calling the added observer functions in the sequence they were added
  /// without checking if the value is changed after applying the provided function.
  pub(crate) fn update_inplace_unchecked(&self, f: impl FnOnce(&mut T)) {
    let mut guard = self.state.value.write();
    let val = guard.deref_mut();
    f(val);

    for obs in self.state.observers.lock().deref_mut() {
      obs(val);
    }
  }

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
    let guard = self.state.value.write();
    let val = guard.deref();
    for obs in self.state.observers.lock().deref_mut() {
      obs(val);
    }
  }
}

impl<T: Debug> Debug for Reactive<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_tuple("Reactive")
      .field(self.state.value.read().deref())
      .finish()
  }
}
