using TrailBase;
using System.Text.Json.Nodes;

public partial class Examples {
  public static async Task<ListResponse<JsonObject>> List(Client client) =>
    await client.Records("movies").List(
        pagination: new Pagination(limit: 3),
        order: ["rank"],
        filters: [
          // Multiple filters on same column: watch_time between 90 and 120 minutes
          new Filter(column:"watch_time", op:CompareOp.GreaterThanOrEqual, value:"90"),
          new Filter(column:"watch_time", op:CompareOp.LessThan, value:"120"),
          // Date range: movies released between 2020 and 2023
          new Filter(column:"release_date", op:CompareOp.GreaterThanOrEqual, value:"2020-01-01"),
          new Filter(column:"release_date", op:CompareOp.LessThanOrEqual, value:"2023-12-31"),
        ]);
}
