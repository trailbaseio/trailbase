import Foundation
import Testing
import TrailBase

@testable import RecordApiDocs

@Test func testDocs() async throws {
  let client = try Client(site: URL(string: "http://localhost:4000")!)
  let _ = try await client.login(email: "admin@localhost", password: "secret")

  let _ = try await list(client: client)

  let id = try await create(client: client)
  try await update(client: client, id: id)
  let record = try await read(client: client, id: id)

  assert(record.text_not_null == "updated")

  try await delete(client: client, id: id)
}
