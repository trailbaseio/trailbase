import Foundation
import Testing

@testable import trailbase

struct SimpleStrict: Codable, Equatable {
  var id: String? = nil

  var text_null: String? = nil
  var text_default: String? = nil
  let text_not_null: String
}

func connect() async throws -> Client {
  let client = try Client(site: URL(string: "http://localhost:4000")!, tokens: nil)
  let _ = try await client.login(email: "admin@localhost", password: "secret")
  return client
}

// TODO: Bootstrap a shared test-local TrailBase instance to make the test hermetic.
@Suite struct Tests {
  @Test func testAuth() async throws {
    let client = try await connect()
    assert(client.tokens?.refresh_token != nil)
    assert(client.user!.email == "admin@localhost")

    try await client.refresh()

    try await client.logout()
    assert(client.tokens == nil)
    assert(client.user == nil)
  }

  @Test func recordTest() async throws {
    let client = try await connect()
    let api = client.records(apiName: "simple_strict_table")

    let now = NSDate().timeIntervalSince1970

    let messages = [
      "swift client test 0: =?&\(now)",
      "swift client test 1: =?&\(now)",
    ]
    var ids: [RecordId] = []

    for message in messages {
      ids.append(try await api.create(record: SimpleStrict(text_not_null: message)))
    }

    // Read
    let record0Read: SimpleStrict = try await api.read(recordId: ids[0])
    assert(record0Read.text_not_null == messages[0])

    // List a specific message
    if true {
      let filter = "text_not_null=\(messages[0])"
      let response: ListResponse<SimpleStrict> = try await api.list(filters: [filter])

      assert(response.records.count == 1)

      let secondResponse: ListResponse<SimpleStrict> = try await api.list(
        pagination: Pagination(cursor: response.cursor), filters: [filter])

      assert(secondResponse.records.count == 0)
    }

    // List all the messages
    if true {
      let filter = "text_not_null[like]=% =?&\(now)"
      let ascending: ListResponse<SimpleStrict> = try await api.list(
        order: ["+text_not_null"], filters: [filter], count: true)

      assert(
        ascending.records.map({ record in
          return record.text_not_null
        }) == messages)
      assert(ascending.total_count == 2)

      let descending: ListResponse<SimpleStrict> = try await api.list(
        order: ["-text_not_null"], filters: [filter], count: true)
      assert(
        descending.records.map({ record in
          return record.text_not_null
        }) == messages.reversed())
      assert(descending.total_count == 2)
    }

    // Update
    let updatedMessage = "swift client updated test 0: =?&\(now)"
    try await api.update(recordId: ids[0], record: SimpleStrict(text_not_null: updatedMessage))
    let record0Update: SimpleStrict = try await api.read(recordId: ids[0])
    assert(record0Update.text_not_null == updatedMessage)

    // Delete
    try await api.delete(recordId: ids[0])
    do {
      let _: SimpleStrict = try await api.read(recordId: ids[0])
      assert(false)
    } catch {
    }
  }
}
