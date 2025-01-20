using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<JsonNode?> Read(Client client, RecordId id) {
    return await client.Records("simple_strict_table").Read<JsonNode>(id);
  }
}
