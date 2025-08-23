import Foundation

public enum Operation: Codable {
    case create(apiName: String, value: [String: AnyCodable])
    case update(apiName: String, recordId: RecordId, value: [String: AnyCodable])
    case delete(apiName: String, recordId: RecordId)

    private enum CodingKeys: String, CodingKey {
        case create = "Create"
        case update = "Update"
        case delete = "Delete"
        case apiName = "api_name"
        case recordId = "record_id"
        case value = "value"
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        switch self {
        case let .create(apiName, value):
            var createContainer = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .create)
            try createContainer.encode(apiName, forKey: .apiName)
            try createContainer.encode(value, forKey: .value)

        case let .update(apiName, recordId, value):
            var updateContainer = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .update)
            try updateContainer.encode(apiName, forKey: .apiName)
            try updateContainer.encode("\(recordId)", forKey: .recordId)
            try updateContainer.encode(value, forKey: .value)

        case let .delete(apiName, recordId):
            var deleteContainer = container.nestedContainer(keyedBy: CodingKeys.self, forKey: .delete)
            try deleteContainer.encode(apiName, forKey: .apiName)
            try deleteContainer.encode("\(recordId)", forKey: .recordId)
        }
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        if let createContainer = try? container.nestedContainer(keyedBy: CodingKeys.self, forKey: .create) {
            let apiName = try createContainer.decode(String.self, forKey: .apiName)
            let value = try createContainer.decode([String: AnyCodable].self, forKey: .value)
            self = .create(apiName: apiName, value: value)

        } else if let updateContainer = try? container.nestedContainer(keyedBy: CodingKeys.self, forKey: .update) {
            let apiName = try updateContainer.decode(String.self, forKey: .apiName)
            let recordIdString = try updateContainer.decode(String.self, forKey: .recordId)
            let value = try updateContainer.decode([String: AnyCodable].self, forKey: .value)
            self = .update(apiName: apiName, recordId: RecordId.string(recordIdString), value: value)

        } else if let deleteContainer = try? container.nestedContainer(keyedBy: CodingKeys.self, forKey: .delete) {
            let apiName = try deleteContainer.decode(String.self, forKey: .apiName)
            let recordIdString = try deleteContainer.decode(String.self, forKey: .recordId)
            self = .delete(apiName: apiName, recordId: RecordId.string(recordIdString))

        } else {
            throw DecodingError.dataCorrupted(
                DecodingError.Context(codingPath: container.codingPath, debugDescription: "Invalid Operation type")
            )
        }
    }
}

public struct TransactionRequest: Codable {
    public var operations: [Operation] = []

    private enum CodingKeys: String, CodingKey {
        case operations = "operations"
    }
}

public struct TransactionResponse: Codable {
    public var ids: [String] = []

    private enum CodingKeys: String, CodingKey {
        case ids = "ids"
    }
}

public class TransactionBatch {
    private let client: Client
    private var operations: [Operation] = []

    init(client: Client) {
        self.client = client
    }

    public func api(_ apiName: String) -> ApiBatch {
        return ApiBatch(batch: self, apiName: apiName)
    }

    public func send() async throws -> [RecordId] {
        let request = TransactionRequest(operations: operations)
        let body = try JSONEncoder().encode(request)

        let (_, data) = try await client.fetch(
            path: "api/transaction/v1/execute",
            method: "POST",
            body: body
        )

        let response = try JSONDecoder().decode(TransactionResponse.self, from: data)
        return response.ids.map { RecordId.string($0) }
    }

    internal func addOperation(_ operation: Operation) {
        operations.append(operation)
    }
}

public class ApiBatch {
    private let batch: TransactionBatch
    private let apiName: String

    init(batch: TransactionBatch, apiName: String) {
        self.batch = batch
        self.apiName = apiName
    }

    public func create(value: [String: AnyCodable]) -> TransactionBatch {
        batch.addOperation(.create(apiName: apiName, value: value))
        return batch
    }

    public func update(recordId: RecordId, value: [String: AnyCodable]) -> TransactionBatch {
        batch.addOperation(.update(apiName: apiName, recordId: recordId, value: value))
        return batch
    }

    public func delete(recordId: RecordId) -> TransactionBatch {
        batch.addOperation(.delete(apiName: apiName, recordId: recordId))
        return batch
    }
}

public struct AnyCodable: Codable {
    public let value: Any

    public init<T>(_ value: T?) {
        self.value = value ?? ()
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self.value = ()
        } else if let bool = try? container.decode(Bool.self) {
            self.value = bool
        } else if let int = try? container.decode(Int.self) {
            self.value = int
        } else if let double = try? container.decode(Double.self) {
            self.value = double
        } else if let string = try? container.decode(String.self) {
            self.value = string
        } else if let array = try? container.decode([AnyCodable].self) {
            self.value = array.map { $0.value }
        } else if let dictionary = try? container.decode([String: AnyCodable].self) {
            self.value = dictionary.mapValues { $0.value }
        } else {
            throw DecodingError.dataCorruptedError(in: container, debugDescription: "AnyCodable value cannot be decoded")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        if value is () {
            try container.encodeNil()
        } else if let bool = value as? Bool {
            try container.encode(bool)
        } else if let int = value as? Int {
            try container.encode(int)
        } else if let double = value as? Double {
            try container.encode(double)
        } else if let string = value as? String {
            try container.encode(string)
        } else if let array = value as? [Any] {
            try container.encode(array.map(AnyCodable.init))
        } else if let dictionary = value as? [String: Any] {
            try container.encode(dictionary.mapValues(AnyCodable.init))
        } else {
            let context = EncodingError.Context(codingPath: container.codingPath, debugDescription: "AnyCodable value cannot be encoded")
            throw EncodingError.invalidValue(value, context)
        }
    }
}
