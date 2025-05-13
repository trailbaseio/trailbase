import TrailBase

func create(client: Client) async throws -> RecordId {
  try await client.records("simple_strict_table").create(
    record: ["text_not_null": "test"])
}
