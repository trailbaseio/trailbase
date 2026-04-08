#![forbid(clippy::unwrap_used)]
#![allow(clippy::needless_return)]
#![warn(
  clippy::await_holding_lock,
  clippy::empty_enums,
  clippy::enum_glob_use,
  clippy::inefficient_to_string,
  clippy::mem_forget,
  clippy::mutex_integer,
  clippy::needless_continue
)]

mod macros;
mod merge;
mod reactive;

pub use merge::Merge;
pub use reactive::Reactive;
