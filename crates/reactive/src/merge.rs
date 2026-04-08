use crate::Reactive;

/// This trait is used for implementing variadic generics.
///
/// The main goal is to convert a tuple (can be implemented for other types too)
/// of reactives of arbitrary size
///     `(&Reactive<usize>, &Reactive<String>, &Reactive<f64>, ...)`
///
/// to a tuple of their inner values
///     `(usize, String, f64, ...)`
///
/// Default implementations for tuples is already provided (see `impl_merge_for_nested_tuple` macro)
/// ```
/// use trailbase_reactive::{Reactive, Merge};
///
/// let r1: Reactive<usize> = Reactive::default();
/// let r2: Reactive<String> = Reactive::default();
/// let r3: Reactive<f64> = Reactive::default();
///
/// let r: Reactive<(usize, String, f64)> = (&r1, &r2, &r3).merge();
///
/// ```
pub trait Merge {
  type Output;

  fn merge(self) -> Reactive<Self::Output>;
}
