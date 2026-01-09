#![allow(clippy::needless_return)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use chrono::TimeZone;
use clap::{CommandFactory, Parser};
use itertools::Itertools;
use serde::Deserialize;
use tokio::{fs, io::AsyncWriteExt};
use trailbase::api::{self, Email, InitArgs, JsonSchemaMode, init_app_state};
use trailbase::{DataDir, Server, ServerOptions, constants::USER_TABLE};
use trailbase_cli::wasm::{
  download_component, find_component, find_component_by_filename, install_wasm_component,
  list_installed_wasm_components, repo,
};
use utoipa::OpenApi;

use trailbase_cli::{
  AdminSubCommands, CommandLineArgs, ComponentReference, ComponentSubCommands, OpenApiSubCommands,
  SubCommands, UserSubCommands,
};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

fn init_logger(dev: bool) {
  const DEFAULT: &str = "info,trailbase_refinery=warn,tracing::span=warn";

  env_logger::Builder::from_env(if dev {
    env_logger::Env::new().default_filter_or(format!("{DEFAULT},trailbase=debug"))
  } else {
    env_logger::Env::new().default_filter_or(DEFAULT)
  })
  .format_timestamp_micros()
  .init();
}

#[derive(Deserialize)]
struct DbUser {
  id: [u8; 16],
  email: String,
  created: i64,
  updated: i64,
}

impl DbUser {
  fn uuid(&self) -> uuid::Uuid {
    uuid::Uuid::from_bytes(self.id)
  }
}

async fn async_main(
  cmd: SubCommands,
  data_dir: DataDir,
  public_url: Option<url::Url>,
  wasm_tokio_runtime: Option<tokio::runtime::Handle>,
) -> Result<(), BoxError> {
  match cmd {
    SubCommands::Run(cmd) => {
      let app = Server::init(ServerOptions {
        data_dir,
        public_url,
        address: cmd.address,
        admin_address: cmd.admin_address,
        public_dir: cmd.public_dir.map(|p| p.into()),
        public_dir_spa: cmd.spa,
        runtime_root_fs: cmd.runtime_root_fs.map(|p| p.into()),
        geoip_db_path: cmd.geoip_db_path.map(|p| p.into()),
        log_responses: cmd.dev || cmd.stderr_logging,
        dev: cmd.dev,
        demo: cmd.demo,
        cors_allowed_origins: cmd.cors_allowed_origins,
        wasm_tokio_runtime,
        tls_key: None,
        tls_cert: None,
      })
      .await?;

      app.serve().await?;
    }
    SubCommands::OpenApi { cmd } => match cmd {
      Some(OpenApiSubCommands::Print) | None => {
        let json = trailbase::openapi::Doc::openapi().to_pretty_json()?;
        println!("{json}");
      }
      #[cfg(feature = "swagger")]
      Some(OpenApiSubCommands::Run { address }) => {
        let router = axum::Router::new().merge(
          utoipa_swagger_ui::SwaggerUi::new("/docs")
            .url("/api/openapi.json", trailbase::openapi::Doc::openapi()),
        );

        let listener = tokio::net::TcpListener::bind(addr.clone()).await.unwrap();
        log::info!("docs @ http://{addr}/docs 🚀");

        axum::serve(listener, router).await.unwrap();
      }
    },
    SubCommands::Schema(cmd) => {
      let (_new_db, state) = init_app_state(InitArgs {
        data_dir,
        public_url,
        ..Default::default()
      })
      .await?;

      let api_name = &cmd.api;
      let Some(api) = state.lookup_record_api(api_name) else {
        return Err(format!("Could not find api: '{api_name}'").into());
      };

      let mode: Option<JsonSchemaMode> = cmd.mode.map(|m| m.into());

      let json_schema = trailbase::api::build_api_json_schema(&state, &api, mode)?;

      println!("{}", serde_json::to_string_pretty(&json_schema)?);
    }
    SubCommands::Migration { suffix, db } => {
      let filename = api::new_unique_migration_filename(suffix.as_deref().unwrap_or("update"));
      let dir = data_dir
        .migrations_path()
        .join(db.as_deref().unwrap_or("main"));
      let path = dir.join(filename);

      if !dir.exists() {
        std::fs::create_dir_all(dir)?;
      }

      let mut migration_file = fs::File::create_new(&path).await?;
      migration_file
        .write_all(b"-- new database migration\n")
        .await?;

      println!("Created empty migration file: {path:?}");
    }
    SubCommands::Admin { cmd } => {
      let (conn, _) = api::init_main_db(Some(&data_dir), None, vec![], vec![])?;

      match cmd {
        Some(AdminSubCommands::List) => {
          let users = conn
            .read_query_values::<DbUser>(format!("SELECT * FROM {USER_TABLE} WHERE admin > 0"), ())
            .await?;

          println!("{: >36}\temail\tcreated\tupdated", "id");
          for user in users {
            let id = user.uuid();

            println!(
              "{id}\t{}\t{created:?}\t{updated:?}",
              user.email,
              created = chrono::Utc.timestamp_opt(user.created, 0),
              updated = chrono::Utc.timestamp_opt(user.updated, 0),
            );
          }
        }
        Some(AdminSubCommands::Demote { user }) => {
          let id = api::cli::demote_admin_to_user(&conn, to_user_reference(user)).await?;
          println!("Demoted admin to user for '{id}'");
        }
        Some(AdminSubCommands::Promote { user }) => {
          let id = api::cli::promote_user_to_admin(&conn, to_user_reference(user)).await?;
          println!("Promoted user to admin for '{id}'");
        }
        None => {
          CommandLineArgs::command()
            .find_subcommand_mut("admin")
            .map(|cmd| cmd.print_help());
        }
      };
    }
    SubCommands::User { cmd } => {
      let (conn, _) = api::init_main_db(Some(&data_dir), None, vec![], vec![])?;

      match cmd {
        Some(UserSubCommands::ChangePassword { user, password }) => {
          let id = api::cli::change_password(&conn, to_user_reference(user), &password).await?;
          println!("Updated password for '{id}'");
        }
        Some(UserSubCommands::ChangeEmail { user, new_email }) => {
          let id = api::cli::change_email(&conn, to_user_reference(user), &new_email).await?;
          println!("Updated email for '{id}'");
        }
        Some(UserSubCommands::Add { email, password }) => {
          api::cli::add_user(&conn, &email, &password).await?;
          println!("Added user '{email}'");
        }
        Some(UserSubCommands::Delete { user }) => {
          api::cli::delete_user(&conn, to_user_reference(user.clone())).await?;
          println!("Deleted user '{user}'");
        }
        Some(UserSubCommands::Verify { user, verified }) => {
          let id = api::cli::set_verified(&conn, to_user_reference(user), verified).await?;
          println!("Set verified={verified} for '{id}'");
        }
        Some(UserSubCommands::InvalidateSession { user }) => {
          api::cli::invalidate_sessions(&conn, to_user_reference(user.clone())).await?;
          println!("Sessions invalidated for '{user}'");
        }
        Some(UserSubCommands::MintToken { user }) => {
          let auth_token =
            api::cli::mint_auth_token(&data_dir, &conn, to_user_reference(user.clone())).await?;
          println!("Bearer {auth_token}");
        }
        None => {
          CommandLineArgs::command()
            .find_subcommand_mut("user")
            .map(|cmd| cmd.print_help());
        }
      };
    }
    SubCommands::Email(cmd) => {
      let (_new_db, state) = init_app_state(InitArgs {
        data_dir,
        public_url,
        ..Default::default()
      })
      .await?;

      let email = Email::new(&state, &cmd.to, cmd.subject, cmd.body)?;
      email.send().await?;

      let c = state.get_config().email;
      match (c.smtp_host, c.smtp_port, c.smtp_username, c.smtp_password) {
        (Some(host), Some(port), Some(username), Some(_)) => {
          println!("Sent email using: {username}@{host}:{port}");
        }
        _ => {
          println!("Sent email using system's sendmail");
        }
      };
    }
    SubCommands::Components { cmd } => {
      match cmd {
        Some(ComponentSubCommands::Add { reference }) => {
          let paths = match ComponentReference::try_from(reference.as_str())? {
            ComponentReference::Name(name) => {
              let component_def = find_component(&name).ok_or("component not found")?;
              let (url, bytes) = download_component(&component_def).await?;

              let filename = url.path();
              install_wasm_component(&data_dir, filename, std::io::Cursor::new(bytes)).await?
            }
            ComponentReference::Url(url) => {
              log::info!("Downloading {url}");
              let bytes = reqwest::get(url.clone()).await?.bytes().await?;
              install_wasm_component(&data_dir, url.path(), std::io::Cursor::new(bytes)).await?
            }
            ComponentReference::Path(path) => {
              let bytes = std::fs::read(&path)?;
              install_wasm_component(&data_dir, &path, std::io::Cursor::new(bytes)).await?
            }
          };

          println!("Added: {paths:?}");
        }
        Some(ComponentSubCommands::Remove { reference }) => {
          match ComponentReference::try_from(reference.as_str())? {
            ComponentReference::Url(_) => {
              return Err("URLs not supported for component removal".into());
            }
            ComponentReference::Name(name) => {
              let component_def = find_component(&name).ok_or("component not found")?;
              let wasm_dir = data_dir.root().join("wasm");

              let filenames: Vec<_> = component_def
                .wasm_filenames
                .into_iter()
                .map(|f| wasm_dir.join(f))
                .collect();

              for filename in &filenames {
                std::fs::remove_file(filename)?;
              }
              println!("Removed: {filenames:?}");
            }
            ComponentReference::Path(path) => {
              std::fs::remove_file(&path)?;

              println!("Removed: {path:?}");
            }
          }
        }
        Some(ComponentSubCommands::List) => {
          println!("Components:\n\n{}", repo().keys().join("\n"));
        }
        Some(ComponentSubCommands::Installed) => {
          let components = list_installed_wasm_components(&data_dir)?;

          for component in components {
            let output = serde_json::to_string_pretty(
              &component
                .packages
                .iter()
                .filter(|p| {
                  return !matches!(p.namespace.as_str(), "wasi" | "root");
                })
                .collect::<Vec<_>>(),
            )?;

            println!(
              "{} - interfaces: {output}",
              component.path.to_string_lossy()
            );
          }
        }
        Some(ComponentSubCommands::Update) => {
          let installed_components = list_installed_wasm_components(&data_dir)?;

          for installed_component in installed_components {
            let Some(filename) = installed_component.path.file_name() else {
              continue;
            };

            let Some(component_def) = find_component_by_filename(&filename.to_string_lossy())
            else {
              log::warn!(
                "Skipping {:?}, not a first-party component",
                installed_component.path
              );
              continue;
            };

            let (url, bytes) = download_component(&component_def).await?;
            let filename = url.path();
            let paths =
              install_wasm_component(&data_dir, filename, std::io::Cursor::new(bytes)).await?;

            println!("Updated : {paths:?}");
          }
        }
        _ => {
          CommandLineArgs::command()
            .find_subcommand_mut("component")
            .map(|cmd| cmd.print_help());
        }
      };
    }
  }

  return Ok(());
}

fn to_user_reference(user: String) -> api::cli::UserReference {
  if user.contains("@") {
    return api::cli::UserReference::Email(user);
  }
  return api::cli::UserReference::Id(user);
}

fn main() -> Result<(), BoxError> {
  let args = CommandLineArgs::parse();

  init_logger(if let Some(SubCommands::Run(ref cmd)) = args.cmd {
    cmd.dev
  } else {
    false
  });

  if args.version {
    let version = trailbase_build::get_version_info!();
    let tag = version.git_version_tag.as_deref().unwrap_or("?");
    let date = version
      .git_commit_date
      .as_deref()
      .unwrap_or_default()
      .trim();

    println!("trail {tag} ({date})");
    println!("sqlite: {}", rusqlite::version());

    return Ok(());
  }

  let Some(cmd) = args.cmd else {
    let _ = CommandLineArgs::command().print_help();
    return Ok(());
  };

  let wasm_tokio_runtime = if let SubCommands::Run(ref cmd) = cmd {
    let num_threads: usize = cmd
      .runtime_threads
      .unwrap_or_else(|| std::thread::available_parallelism().map_or(0, |n| n.into()));

    if num_threads > 0 {
      log::info!("Created dedicated WASM runtime with {num_threads} threads");
      Some(
        tokio::runtime::Builder::new_multi_thread()
          .thread_name("WASM")
          .worker_threads(num_threads)
          .enable_all()
          .build()?,
      )
    } else {
      None
    }
  } else {
    None
  };

  let main_runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;

  main_runtime.block_on(async_main(
    cmd,
    DataDir(args.data_dir.clone()),
    args.public_url,
    wasm_tokio_runtime.as_ref().map(|rt| rt.handle().clone()),
  ))?;

  if let Some(wasm_tokio_runtime) = wasm_tokio_runtime {
    wasm_tokio_runtime.shutdown_timeout(tokio::time::Duration::from_secs(10));
  }

  return Ok(());
}
