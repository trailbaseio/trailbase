import Foundation

public enum Operation: Equatable, Encodable, Sendable {
  case Create(api_name: String, value: [String: JSON])
  case Update(api_name: String, record_id: String, value: [String: JSON])
  case Delete(api_name: String, record_id: String)
}

public enum OperationResult: Sendable {
  case Id(RecordId)
  case Error(String)
}
