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

    var id = await Examples.Create(client);

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
  }
}
