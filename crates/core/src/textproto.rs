use prost::Message;

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("Decode: {0}")]
  Decode(#[from] prost::DecodeError),
  #[error("Parse: {0}")]
  Parse(#[from] prost_reflect::text_format::ParseError),
}

pub trait Textproto<M: Message> {
  fn from_text(text: &str) -> Result<M, Error>;
  fn to_text(&self) -> Result<String, Error>;
}
