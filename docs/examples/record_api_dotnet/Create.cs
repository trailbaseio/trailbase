using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<RecordId> Create(Client client) =>
    await client.Records("simple_strict_table").Create(
        new JsonObject { ["text_not_null"] = "test" });
}
