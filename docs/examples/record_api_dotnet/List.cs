using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<ListResponse<JsonObject>> List(Client client) =>
    await client.Records("movies").List(
        pagination: new Pagination(limit: 3),
        order: ["rank"],
        filters: [
          new Filter(column:"watch_time", op:CompareOp.LessThan, value:"120"),
          new Filter(column:"description", op:CompareOp.Like, value:"%love%"),
        ]);
}
