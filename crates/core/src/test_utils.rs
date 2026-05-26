pub fn strict() -> &'static str {
  return cfg_select! {
      feature = "pg" => "",
      _ => "STRICT",
  };
}
