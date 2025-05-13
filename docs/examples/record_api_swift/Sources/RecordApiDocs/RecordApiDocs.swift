struct SimpleStrict: Codable, Equatable {
  var id: String? = nil

  var text_null: String? = nil
  var text_default: String? = nil
  let text_not_null: String
}
