import TrailBase

func delete(client: Client, id: RecordId) async throws {
  try await client.records("simple_strict_table").delete(recordId: id)
}
