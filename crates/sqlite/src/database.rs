#[derive(Clone, Debug, PartialEq, serde::Deserialize)]
pub struct Database {
  pub seq: u8,
  pub name: String,
}
