#![allow(clippy::needless_return)]

mod args;

pub use args::{
  AdminSubCommands, DefaultCommandLineArgs, EmailArgs, JsonSchemaModeArg, SubCommands,
  UserSubCommands,
};

#[cfg(feature = "openapi")]
pub use args::OpenApiSubCommands;
