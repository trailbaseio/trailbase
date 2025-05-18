import Foundation
import Testing
import TrailBase

@testable import RecordApiDocs

struct SimpleStrict: Codable, Equatable {
    var id: String? = nil

    var text_null: String? = nil
    var text_default: String? = nil
    let text_not_null: String
}

@Test func testDocs() async throws {
    let client = try Client(site: URL(string: "http://localhost:4000")!)
    let _ = try await client.login(email: "admin@localhost", password: "secret")

    let movies = try await list(client: client)
    #expect(movies.records.count == 3)

    let id = try await create(client: client)
    try await update(client: client, id: id)
    let record = try await read(client: client, id: id)

    #expect(record.text_not_null == "updated")

    try await delete(client: client, id: id)
}
