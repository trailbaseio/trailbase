using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<RecordId> Create(Client client) {
    return await client.Records("simple_strict_table").Create(JsonNode.Parse("""{"text_not_null": "test"}"""));
  }
}
