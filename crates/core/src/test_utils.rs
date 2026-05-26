pub fn strict() -> &'static str {
  return cfg_select! {
      feature = "pg" => "",
      _ => "STRICT",
  };
}

pub fn uuid_column() -> &'static str {
  return cfg_select! {
      feature = "pg" => "UUID",
      _ => "BLOB",
  };
}
