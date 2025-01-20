using TrailBase;

public partial class Examples {
  public static async Task Delete(Client client, RecordId id) {
    await client.Records("simple_strict_table").Delete(id);
  }
}
