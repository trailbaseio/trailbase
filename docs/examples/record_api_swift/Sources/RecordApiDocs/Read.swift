import TrailBase

func read(client: Client, id: RecordId) async throws -> SimpleStrict {
    try await client.records("simple_strict_table").read(recordId: id)
}
