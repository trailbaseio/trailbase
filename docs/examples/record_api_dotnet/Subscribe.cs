using TrailBase;

public partial class Examples {
  public static async Task<IAsyncEnumerable<Event>> Subscribe(Client client, RecordId id) =>
    await client.Records("simple_strict_table").Subscribe(id);

  public static async Task<IAsyncEnumerable<Event>> SubscribeAll(Client client) =>
    await client.Records("simple_strict_table").SubscribeAll();
}
