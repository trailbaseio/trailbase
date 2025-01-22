using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<ListResponse<JsonObject>> List(Client client) =>
    await client.Records("movies").List(
        pagination: new Pagination(limit: 3),
        order: ["rank"],
        filters: ["watch_time[lt]=120", "description[like]=%love%"]);
}
