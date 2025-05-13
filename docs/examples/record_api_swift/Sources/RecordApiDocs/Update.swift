import TrailBase

func update(client: Client, id: RecordId) async throws {
  try await client.records("simple_strict_table").update(
    recordId: id, record: ["text_not_null": "updated"])
}
