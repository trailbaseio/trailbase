using Xunit;
using TrailBase;

public class ExamplesTestFixture : IDisposable {
  public ExamplesTestFixture() { }

  public void Dispose() { }
}

public class ExamplesTest : IClassFixture<ExamplesTestFixture> {
  ExamplesTestFixture fixture;

  public ExamplesTest(ExamplesTestFixture fixture) {
    this.fixture = fixture;
  }

  public async Task<Client> Connect() {
    var client = new Client("http://localhost:4000", null);
    await client.Login("admin@localhost", "secret");
    return client;
  }

  [Fact]
  public async Task BasicTest() {
    var client = await Connect();

    var tableStream = await Examples.SubscribeAll(client);

    var id = await Examples.Create(client);

    var recordStream = await Examples.Subscribe(client, id);

    Console.WriteLine($"ID: {id}");

    {
      var record = await Examples.Read(client, id);
      Console.WriteLine($"ID: {record}");
      Assert.Equal("test", record!["text_not_null"]!.ToString());
    }

    {
      await Examples.Update(client, id);
      var record = await Examples.Read(client, id);
      Assert.Equal("updated", record!["text_not_null"]!.ToString());
    }

    await Examples.Delete(client, id);

    List<Event> events = [];
    await foreach (Event ev in recordStream) {
      events.Add(ev);
    }
    Assert.Equal(2, events.Count);

    List<Event> tableEvents = [];
    await foreach (Event ev in tableStream) {
      tableEvents.Add(ev);

      if (tableEvents.Count >= 3) {
        break;
      }
    }
  }

  [Fact]
  public async Task ListTest() {
    var client = await Connect();
    var response = await Examples.List(client);

    Assert.Equal(3, response.records.Count);
    Assert.Equal("Casablanca", response.records[0]["name"]!.ToString());
  }
}
