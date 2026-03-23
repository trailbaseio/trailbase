import Foundation

/// A JSON value representation from:
///   https://github.com/iwill/generic-json-swift/blob/master/GenericJSON/JSON.swift.
/// This is a bit more useful than the naïve `[String:Any]` type for JSON values,
/// since it makes sure only valid JSON values are present & supports `Equatable`
/// and `Decodable`.
public enum JSON: Equatable, Sendable {
  case string(String)
  case number(Double)
  case object([String: JSON])
  case array([JSON])
  case bool(Bool)
  case null
}

extension JSON: Codable {
  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()

    switch self {
    case .array(let array):
      try container.encode(array)
    case .object(let object):
      try container.encode(object)
    case .string(let string):
      try container.encode(string)
    case .number(let number):
      try container.encode(number)
    case .bool(let bool):
      try container.encode(bool)
    case .null:
      try container.encodeNil()
    }
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()

    if let object = try? container.decode([String: JSON].self) {
      self = .object(object)
    } else if let array = try? container.decode([JSON].self) {
      self = .array(array)
    } else if let string = try? container.decode(String.self) {
      self = .string(string)
    } else if let bool = try? container.decode(Bool.self) {
      self = .bool(bool)
    } else if let number = try? container.decode(Double.self) {
      self = .number(number)
    } else if container.decodeNil() {
      self = .null
    } else {
      throw DecodingError.dataCorrupted(
        .init(codingPath: decoder.codingPath, debugDescription: "Invalid JSON value.")
      )
    }
  }
}

extension [String: JSON] {
  public func decodeValue<T: Decodable>() throws -> T? {
    // Not very efficient. Would love some best practices.
    let encoded = try JSONEncoder().encode(self)
    return try JSONDecoder().decode(T.self, from: encoded)
  }
}

private struct EventRepr {
  let type: String
  let seq: UInt32?
  let value: [String: JSON]?
  let error: String?

  enum CodingKeys: String, CodingKey {
    case type, seq, value, error
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.type = try container.decode(String.self, forKey: .type)
    self.seq = try? container.decode(UInt32?.self, forKey: .seq)
    self.value = try? container.decode([String: JSON]?.self, forKey: .value)
    self.error = try? container.decode(String?.self, forKey: .error)
  }
}

public enum Event: Equatable, Decodable, Sendable {
  case Insert(type: String, seq: UInt32?, value: [String: JSON])
  case Update(type: String, seq: UInt32?, value: [String: JSON])
  case Delete(type: String, seq: UInt32?, value: [String: JSON])
  case Error(type: String, seq: UInt32?, error: String)

  public init(from decoder: Decoder) throws {
    let repr = try EventRepr(from: decoder)

    switch repr.type {
    case "insert": self = .Insert(type: "insert", seq: repr.seq, value: repr.value!)
    case "update": self = .Update(type: "update", seq: repr.seq, value: repr.value!)
    case "delete": self = .Insert(type: "delete", seq: repr.seq, value: repr.value!)
    case "error": self = .Error(type: "error", seq: repr.seq, error: repr.error!)
    default:
      throw ClientError.invalidEvent
    }
  }

  public func decodeValue<T: Decodable>() throws -> T? {
    switch self {
    case .Insert(_, _, let value), .Update(_, _, let value), .Delete(_, _, let value):
      return try value.decodeValue()
    default:
      return nil
    }
  }
}
