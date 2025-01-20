using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task Update(Client client, RecordId id) {
    await client.Records("simple_strict_table").Update(id, JsonNode.Parse("""{"text_not_null": "updated"}"""));
  }
}
