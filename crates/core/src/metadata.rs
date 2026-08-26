use log::*;
use tokio::fs;
use trailbase_build::version::GitVersion;

use crate::config::ConfigError;
use crate::data_dir::DataDir;

pub mod proto {
  use lazy_static::lazy_static;
  use prost_reflect::text_format::FormatOptions;
  use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage};

  use crate::DESCRIPTOR_POOL;
  use crate::config::ConfigError;

  include!(concat!(env!("OUT_DIR"), "/metadata.rs"));

  lazy_static! {
    static ref METADATA_DESCRIPTOR: MessageDescriptor = DESCRIPTOR_POOL
      .get_message_by_name("metadata.Metadata")
      .expect("infallible");
  }

  impl Metadata {
    pub fn new_with_custom_defaults() -> Self {
      let version_info = trailbase_build::get_version_info!();
      return Self {
        last_executed_version: version_info.git_version_tag,
      };
    }

    pub fn from_text(text: &str) -> Result<Self, ConfigError> {
      let dyn_config = DynamicMessage::parse_text_format(METADATA_DESCRIPTOR.clone(), text)?;
      return Ok(dyn_config.transcode_to::<Self>()?);
    }

    pub fn to_text(&self) -> Result<String, ConfigError> {
      const PREAMBLE: &str = "# Auto-generated `metadata.Metadata` textproto.\n#\n# Schema: https://github.com/trailbaseio/trailbase/blob/main/crates/core/proto/metadata.proto";

      let text: String = self
        .transcode_to_dynamic()
        .to_text_format_with_options(&FormatOptions::new().pretty(true).expand_any(true));

      return Ok(format!("{PREAMBLE}\n{text}"));
    }
  }
}

pub async fn load_check_and_update_metadata_textproto(
  data_dir: &DataDir,
) -> Result<(), ConfigError> {
  let metadata_path = data_dir.config_path().join(METADATA_FILENAME);

  let loaded = match fs::read_to_string(&metadata_path).await {
    Ok(contents) => proto::Metadata::from_text(&contents)?,
    Err(err) => match err.kind() {
      std::io::ErrorKind::NotFound => proto::Metadata::new_with_custom_defaults(),
      _ => return Err(err.into()),
    },
  };
  let loaded_version = GitVersion::parse(loaded.last_executed_version())
    .ok_or_else(|| ConfigError::Invalid("failed to parse version".into()))?;

  let current = proto::Metadata::new_with_custom_defaults();
  let current_version = GitVersion::parse(current.last_executed_version())
    .ok_or_else(|| ConfigError::Invalid("failed to parse version".into()))?;

  if (loaded_version.major == 0 && loaded_version.minor > current_version.minor)
    || loaded_version.major > current_version.major
  {
    warn!(
      "Running a potentially incompatible version version: {current_version} (previously: {loaded_version})"
    );
  }

  if current_version >= loaded_version {
    debug!("Update metadata.textproto: {metadata_path:?}");
    fs::write(&metadata_path, current.to_text()?.as_bytes()).await?;
  }

  return Ok(());
}

const METADATA_FILENAME: &str = "metadata.textproto";
