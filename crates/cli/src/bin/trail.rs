#![allow(clippy::needless_return)]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use chrono::TimeZone;
use clap::{CommandFactory, Parser};
use serde::Deserialize;
use std::rc::Rc;
use tokio::{fs, io::AsyncWriteExt};
use trailbase::{
  DataDir, Server, ServerOptions,
  api::{self, Email, InitArgs, JsonSchemaMode, init_app_state},
  constants::USER_TABLE,
};
use utoipa::OpenApi;

use trailbase_cli::{
  AdminSubCommands, DefaultCommandLineArgs, OpenApiSubCommands, SubCommands, UserSubCommands,
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

async fn async_main() -> Result<(), BoxError> {
  let args = DefaultCommandLineArgs::parse();

  if args.version {
    let version = trailbase_build::get_version_info!();
    let tag = version.version_tag.as_deref().unwrap_or("?");
    let hash = version.commit_hash.as_deref().unwrap_or_default().trim();
    let date = version.commit_date.as_deref().unwrap_or_default().trim();

    println!("trail {tag} ({hash} {date})");

    return Ok(());
  }

  let data_dir = DataDir(args.data_dir.clone());

  match args.cmd {
    Some(SubCommands::Run(cmd)) => {
      init_logger(cmd.dev);

      let app = Server::init(ServerOptions {
        data_dir,
        public_url: args.public_url,
        address: cmd.address,
        admin_address: cmd.admin_address,
        public_dir: cmd.public_dir.map(|p| p.into()),
        wasm_root_dir: cmd.wasm_root_dir.map(|p| p.into()),
        geoip_db_path: cmd.geoip_db_path.map(|p| p.into()),
        log_responses: cmd.dev || cmd.stderr_logging,
        dev: cmd.dev,
        demo: cmd.demo,
        disable_auth_ui: cmd.disable_auth_ui,
        cors_allowed_origins: cmd.cors_allowed_origins,
        js_runtime_threads: cmd.js_runtime_threads,
        tls_key: None,
        tls_cert: None,
      })
      .await?;

      app.serve().await?;
    }
    Some(SubCommands::OpenApi { cmd }) => {
      init_logger(false);

      match cmd {
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
      }
    }
    Some(SubCommands::Schema(cmd)) => {
      init_logger(false);

      let (_new_db, state) = init_app_state(InitArgs {
        data_dir: DataDir(args.data_dir),
        public_url: args.public_url,
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
    Some(SubCommands::Migration { suffix }) => {
      init_logger(false);

      let filename = api::new_unique_migration_filename(suffix.as_deref().unwrap_or("update"));
      let path = data_dir.migrations_path().join(filename);

      let mut migration_file = fs::File::create_new(&path).await?;
      migration_file
        .write_all(b"-- new database migration\n")
        .await?;

      println!("Created empty migration file: {path:?}");
    }
    Some(SubCommands::Admin { cmd }) => {
      init_logger(false);

      let (conn, _) = api::init_main_db(Some(&data_dir), None)?;

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
          DefaultCommandLineArgs::command()
            .find_subcommand_mut("admin")
            .map(|cmd| cmd.print_help());
        }
      };
    }
    Some(SubCommands::User { cmd }) => {
      init_logger(false);

      let data_dir = DataDir(args.data_dir);
      let (conn, _) = api::init_main_db(Some(&data_dir), None)?;

      match cmd {
        Some(UserSubCommands::ChangePassword { user, password }) => {
          let id = api::cli::change_password(&conn, to_user_reference(user), &password).await?;
          println!("Updated password for '{id}'");
        }
        Some(UserSubCommands::ChangeEmail { user, new_email }) => {
          let id = api::cli::change_email(&conn, to_user_reference(user), &new_email).await?;
          println!("Updated email for '{id}'");
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
          DefaultCommandLineArgs::command()
            .find_subcommand_mut("user")
            .map(|cmd| cmd.print_help());
        }
      };
    }
    Some(SubCommands::Email(cmd)) => {
      init_logger(false);

      let (_new_db, state) = init_app_state(InitArgs {
        data_dir: DataDir(args.data_dir),
        public_url: args.public_url,
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
    None => {
      let _ = DefaultCommandLineArgs::command().print_help();
    }
  }

  Ok(())
}

fn to_user_reference(user: String) -> api::cli::UserReference {
  if user.contains("@") {
    return api::cli::UserReference::Email(user);
  }
  return api::cli::UserReference::Id(user);
}

fn main() -> Result<(), BoxError> {
  let runtime = Rc::new(
    tokio::runtime::Builder::new_multi_thread()
      .enable_all()
      .build()?,
  );
  return runtime.block_on(async_main());
}
