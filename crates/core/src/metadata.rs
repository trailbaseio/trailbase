use log::*;
use tokio::fs;

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
        last_executed_version: version_info.version_tag,
      };
    }

    pub fn from_text(text: &str) -> Result<Self, ConfigError> {
      let dyn_config = DynamicMessage::parse_text_format(METADATA_DESCRIPTOR.clone(), text)?;
      return Ok(dyn_config.transcode_to::<Self>()?);
    }

    pub fn to_text(&self) -> Result<String, ConfigError> {
      const PREFACE: &str = "# Auto-generated metadata.Metadata textproto";

      let text: String = self
        .transcode_to_dynamic()
        .to_text_format_with_options(&FormatOptions::new().pretty(true).expand_any(true));

      return Ok(format!("{PREFACE}\n{text}"));
    }
  }
}

pub async fn load_or_init_metadata_textproto(
  data_dir: &DataDir,
) -> Result<proto::Metadata, ConfigError> {
  let metadata_path = data_dir.config_path().join(METADATA_FILENAME);

  let config: proto::Metadata = match fs::read_to_string(&metadata_path).await {
    Ok(contents) => proto::Metadata::from_text(&contents)?,
    Err(err) => match err.kind() {
      std::io::ErrorKind::NotFound => {
        warn!("Falling back to default config: {err}");
        let config = proto::Metadata::new_with_custom_defaults();

        debug!("Writing metadata: {metadata_path:?}");
        fs::write(&metadata_path, config.to_text()?.as_bytes()).await?;

        config
      }
      _ => {
        return Err(err.into());
      }
    },
  };

  return Ok(config);
}

const METADATA_FILENAME: &str = "metadata.textproto";
