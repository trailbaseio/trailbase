import Foundation
import FoundationNetworking
import Subprocess
import SystemPackage
import Testing

@testable import TrailBase

let PORT: UInt16 = 4058

func panic(_ msg: String) -> Never {
    print("ABORT: \(msg)", FileHandle.standardError)
    abort()
}

struct SimpleStrict: Codable, Equatable {
    var id: String? = nil

    var text_null: String? = nil
    var text_default: String? = nil
    let text_not_null: String
}

func connect() async throws -> Client {
    let client = try Client(site: URL(string: "http://127.0.0.1:\(PORT)")!, tokens: nil)
    let _ = try await client.login(email: "admin@localhost", password: "secret")
    return client
}

public enum StartupError: Error {
    case configNotFound(path: String)
    case buildFailed(stdout: String?, stderr: String?)
    case startupTimeout
}

func startTrailBase() async throws -> ProcessIdentifier {
    let cwd = FilePath("../../../")
    let depotPath = "client/testfixture"

    let traildepot = cwd.appending(depotPath).string
    if !FileManager.default.fileExists(atPath: traildepot) {
        throw StartupError.configNotFound(path: traildepot)
    }

    let build = try await Subprocess.run(
        .name("cargo"), arguments: ["build"], workingDirectory: cwd, output: .string, error: .string
    )

    if !build.terminationStatus.isSuccess {
        throw StartupError.buildFailed(stdout: build.standardOutput, stderr: build.standardError)
    }

    let arguments: Arguments = [
        "run",
        "--",
        "--data-dir=\(depotPath)",
        "run",
        "--address=127.0.0.1:\(PORT)",
        "--js-runtime-threads=2",
    ]

    let process = try Subprocess.runDetached(
        .name("cargo"),
        arguments: arguments,
        workingDirectory: cwd,
        output: .standardOutput,
        error: .standardError,
    )

    // Make sure it's up and running.
    let request = URLRequest(url: URL(string: "http://127.0.0.1:\(PORT)/api/healthcheck")!)
    for _ in 0...100 {
        do {
            let (data, _) = try await URLSession.shared.data(for: request)
            let body = String(data: data, encoding: .utf8)!
            if body.uppercased() == "OK" {
                print("Started TrailBase")
                return process
            }
        } catch {
        }

        usleep(500 * 1000)
    }

    kill(process.value, SIGKILL)

    throw StartupError.startupTimeout
}

final class SetupTrailBaseTrait: SuiteTrait, TestScoping {
    // Only apply to Suite and not recursively to tests (also is default).
    public var isRecursive: Bool { false }

    func provideScope(
        for test: Test,
        testCase: Test.Case?,
        performing: () async throws -> Void
    ) async throws {
        // Setup
        print("Starting TrailBase \(test.name)")
        let process = try await startTrailBase()

        // Run the actual test suite, i.e. all tests:
        do {
            try await performing()
        } catch {
        }

        // Tear-down
        print("Killing TrailBase \(test.name)")
        kill(process.value, SIGKILL)
    }
}

extension Trait where Self == SetupTrailBaseTrait {
    static var setupTrailBase: Self { Self() }
}

@Suite(.setupTrailBase) struct ClientTestSuite {
    @Test("Test Authentication") func testAuth() async throws {
        let client = try await connect()
        #expect(client.tokens?.refresh_token != nil)
        #expect(client.user!.email == "admin@localhost")

        try await client.refresh()

        try await client.logout()
        #expect(client.tokens == nil)
        #expect(client.user == nil)
    }

    @Test func recordTest() async throws {
        let client = try await connect()
        let api = client.records("simple_strict_table")

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
            let filter = Filter.Filter(column: "text_not_null", value: messages[0])
            let response: ListResponse<SimpleStrict> = try await api.list(filters: [filter])

            assert(response.records.count == 1)

            let secondResponse: ListResponse<SimpleStrict> = try await api.list(
                pagination: Pagination(cursor: response.cursor), filters: [filter])

            assert(secondResponse.records.count == 0)
        }

        // List all the messages
        if true {
            let filter = Filter.Filter(
                column: "text_not_null", op: CompareOp.Like, value: "% =?&\(now)")
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

    @Test("Test Transaction") func testTransaction() async throws {
        let client = try await connect()
        let now = NSDate().timeIntervalSince1970

        // Test create operation
        let batch = client.transaction()
        let createRecord: [String: AnyCodable] = [
            "text_not_null": AnyCodable("swift transaction create test: =?&\(now)")
        ]
        batch.api("simple_strict_table").create(value: createRecord)

        // Test actual creation
        let ids: [RecordId] = try await batch.send()
        #expect(ids.count == 1)

        // Verify record was created
        let api = client.records("simple_strict_table")
        let createdRecord: SimpleStrict = try await api.read(recordId: ids[0])
        #expect(createdRecord.text_not_null == createRecord["text_not_null"]?.value as? String)

        // Test update operation
        let updateBatch = client.transaction()
        let updateRecord: [String: AnyCodable] = [
            "text_not_null": AnyCodable("swift transaction update test: =?&\(now)")
        ]
        updateBatch.api("simple_strict_table").update(recordId: ids[0], value: updateRecord)

        // Test actual update
        let _ = try await updateBatch.send()
        let updatedRecord: SimpleStrict = try await api.read(recordId: ids[0])
        #expect(updatedRecord.text_not_null == updateRecord["text_not_null"]?.value as? String)

        // Test delete operation
        let deleteBatch = client.transaction()
        deleteBatch.api("simple_strict_table").delete(recordId: ids[0])

        // Test actual deletion
        let _ = try await deleteBatch.send()
        do {
            let _: SimpleStrict = try await api.read(recordId: ids[0])
            #expect(Bool(false))
        } catch {
        }
    }
}
