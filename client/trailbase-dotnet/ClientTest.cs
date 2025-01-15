using Xunit;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Serialization;
using System.Diagnostics.CodeAnalysis;

namespace TrailBase;

static class Constants {
  public static int Port = 4010 + System.Environment.Version.Major;
}

class SimpleStrict {
  public string? id { get; }

  public string? text_null { get; }
  public string? text_default { get; }
  public string text_not_null { get; }

  public SimpleStrict(string? id, string? text_null, string? text_default, string text_not_null) {
    this.id = id;
    this.text_null = text_null;
    this.text_default = text_default;
    this.text_not_null = text_not_null;
  }
}


[JsonSourceGenerationOptions(WriteIndented = true, DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull)]
[JsonSerializable(typeof(SimpleStrict))]
[JsonSerializable(typeof(List<SimpleStrict>))]
internal partial class SerializeSimpleStrictContext : JsonSerializerContext {
}

public class ClientTestFixture : IDisposable {
  Process process;

  public ClientTestFixture() {
    string projectDirectory = Directory.GetParent(Environment.CurrentDirectory)!.Parent!.Parent!.FullName;

    Console.WriteLine($"Building TrailBase: {projectDirectory}");
    var buildProcess = new Process();
    buildProcess.StartInfo.WorkingDirectory = projectDirectory;
    buildProcess.StartInfo.FileName = "cargo";
    buildProcess.StartInfo.Arguments = "build";
    buildProcess.StartInfo.UseShellExecute = false;
    buildProcess.StartInfo.RedirectStandardOutput = true;
    buildProcess.Start();
    var exited = buildProcess.WaitForExit(TimeSpan.FromMinutes(10));
    if (!exited) {
      buildProcess.Kill();
    }

    var address = $"127.0.0.1:{Constants.Port}";
    Console.WriteLine($"Starting TrailBase: {address}: {projectDirectory}");

    process = new Process();
    process.StartInfo.WorkingDirectory = projectDirectory;
    process.StartInfo.FileName = "cargo";
    process.StartInfo.Arguments = $"run -- --data-dir ../testfixture run -a {address} --js-runtime-threads 2";
    process.StartInfo.UseShellExecute = false;
    process.StartInfo.RedirectStandardOutput = true;
    process.Start();

    var client = new HttpClient();
    Task.Run(async () => {
      for (int i = 0; i < 100; ++i) {
        try {
          var response = await client.GetAsync($"http://{address}/api/healthcheck");
          if (response.StatusCode == System.Net.HttpStatusCode.OK) {
            break;
          }
        }
        catch (Exception e) {
          Console.WriteLine($"Caught exception: {e.Message}");
        }

        await Task.Delay(500);
      }
    }).Wait();
  }

  public void Dispose() {
    process.Kill();
  }
}

public class ClientTest : IClassFixture<ClientTestFixture> {
  ClientTestFixture fixture;

  public ClientTest(ClientTestFixture fixture) {
    this.fixture = fixture;
  }

  [Fact]
  public void IdTest() {
    var integerId = new IntegerRecordId(42);
    Assert.Equal("42", integerId.ToString());
  }

  [Fact]
  public async Task AuthTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    var oldTokens = await client.Login("admin@localhost", "secret");
    Assert.NotNull(oldTokens?.auth_token);
    var user = client.User();
    Assert.NotNull(user);
    Assert.Equal("admin@localhost", user!.email);
    Assert.True(user!.sub != "");

    await client.Logout();

    await Task.Delay(1500);
    var newTokens = await client.Login("admin@localhost", "secret");

    Assert.NotEqual(newTokens?.auth_token, oldTokens?.auth_token);
  }

  [Fact]
  [RequiresDynamicCode("Testing dynamic code")]
  [RequiresUnreferencedCode("testing dynamic code")]
  public async Task RecordsTestDynamic() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    await client.Login("admin@localhost", "secret");

    var api = client.Records("simple_strict_table");

    var now = DateTimeOffset.Now.ToUnixTimeSeconds();

    // Dotnet runs tests for multiple target framework versions in parallel.
    // Each test currently brings up its own server but pointing at the same
    // underlying database file. We include the runtime version in the filter
    // query to avoid a race between both tests. This feels a bit hacky.
    // Ideally, we'd run the tests sequentially or with better isolation :/.
    var suffix = $"{now} {System.Environment.Version} dyn";
    List<string> messages = [
      $"C# client test 0:  =?&{suffix}",
      $"C# client test 1:  =?&{suffix}",
    ];

    List<RecordId> ids = [];
    foreach (var msg in messages) {
      ids.Add(await api.Create(new SimpleStrict(null, null, null, msg)));
    }

    {
      List<SimpleStrict> records = await api.List<SimpleStrict>(
        null,
        null,
        [$"text_not_null={messages[0]}"]
      )!;
      Assert.Single(records);
      Assert.Equal(messages[0], records[0].text_not_null);
    }

    {
      var recordsAsc = await api.List<SimpleStrict>(
        null,
        ["+text_not_null"],
        [$"text_not_null[like]=% =?&{suffix}"]
      )!;
      Assert.Equal(messages.Count, recordsAsc.Count);
      Assert.Equal(messages, recordsAsc.ConvertAll((e) => e.text_not_null));

      var recordsDesc = await api.List<SimpleStrict>(
        null,
        ["-text_not_null"],
        [$"text_not_null[like]=%{suffix}"]
      )!;
      Assert.Equal(messages.Count, recordsDesc.Count);
      recordsDesc.Reverse();
      Assert.Equal(messages, recordsDesc.ConvertAll((e) => e.text_not_null));
    }

    var i = 0;
    foreach (var id in ids) {
      var msg = messages[i++];
      var record = await api.Read<SimpleStrict>(id);

      Assert.Equal(msg, record!.text_not_null);
      Assert.NotNull(record.id);

      var uuidId = new UuidRecordId(record.id!);
      Assert.Equal(id.ToString(), uuidId.ToString());
    }

    {
      var id = ids[0];
      var msg = $"{messages[0]} - updated";
      await api.Update(id, new SimpleStrict(null, null, null, msg));
      var record = await api.Read<SimpleStrict>(id);

      Assert.Equal(msg, record!.text_not_null);
    }

    {
      var id = ids[0];
      await api.Delete(id);

      var records = await api.List<SimpleStrict>(
        null,
        null,
        [$"text_not_null[like]=%{suffix}"]
      )!;

      Assert.Single(records);
    }
  }

  [Fact]
  public async Task RecordsTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    await client.Login("admin@localhost", "secret");

    var api = client.Records("simple_strict_table");

    var now = DateTimeOffset.Now.ToUnixTimeSeconds();

    // Dotnet runs tests for multiple target framework versions in parallel.
    // Each test currently brings up its own server but pointing at the same
    // underlying database file. We include the runtime version in the filter
    // query to avoid a race between both tests. This feels a bit hacky.
    // Ideally, we'd run the tests sequentially or with better isolation :/.
    var suffix = $"{now} {System.Environment.Version} static";
    List<string> messages = [
      $"C# client test 0:  =?&{suffix}",
      $"C# client test 1:  =?&{suffix}",
    ];

    List<RecordId> ids = [];
    foreach (var msg in messages) {
      ids.Add(await api.Create(new SimpleStrict(null, null, null, msg), SerializeSimpleStrictContext.Default.SimpleStrict));
    }

    {
      List<SimpleStrict> records = await api.List(
        null,
        null,
        [$"text_not_null={messages[0]}"],
        SerializeSimpleStrictContext.Default.ListSimpleStrict
      )!;
      Assert.Single(records);
      Assert.Equal(messages[0], records[0].text_not_null);
    }

    {
      var recordsAsc = await api.List(
        null,
        ["+text_not_null"],
        [$"text_not_null[like]=% =?&{suffix}"],
        SerializeSimpleStrictContext.Default.ListSimpleStrict
      )!;
      Assert.Equal(messages.Count, recordsAsc.Count);
      Assert.Equal(messages, recordsAsc.ConvertAll((e) => e.text_not_null));

      var recordsDesc = await api.List(
        null,
        ["-text_not_null"],
        [$"text_not_null[like]=%{suffix}"],
        SerializeSimpleStrictContext.Default.ListSimpleStrict
      )!;
      Assert.Equal(messages.Count, recordsDesc.Count);
      recordsDesc.Reverse();
      Assert.Equal(messages, recordsDesc.ConvertAll((e) => e.text_not_null));
    }

    var i = 0;
    foreach (var id in ids) {
      var msg = messages[i++];
      var record = await api.Read(id, SerializeSimpleStrictContext.Default.SimpleStrict);

      Assert.Equal(msg, record!.text_not_null);
      Assert.NotNull(record.id);

      var uuidId = new UuidRecordId(record.id!);
      Assert.Equal(id.ToString(), uuidId.ToString());
    }

    {
      var id = ids[0];
      var msg = $"{messages[0]} - updated";
      await api.Update(
        id,
        new SimpleStrict(null, null, null, msg),
        SerializeSimpleStrictContext.Default.SimpleStrict
      );
      var record = await api.Read(id, SerializeSimpleStrictContext.Default.SimpleStrict);

      Assert.Equal(msg, record!.text_not_null);
    }

    {
      var id = ids[0];
      await api.Delete(id);

      var records = await api.List(
        null,
        null,
        [$"text_not_null[like]=%{suffix}"],
        SerializeSimpleStrictContext.Default.ListSimpleStrict
      )!;

      Assert.Single(records);
    }
  }

  [Fact]
  public async Task RealtimeTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    await client.Login("admin@localhost", "secret");

    var api = client.Records("simple_strict_table");

    var tableEventStream = await api.SubscribeAll();

    // Dotnet runs tests for multiple target framework versions in parallel.
    // Each test currently brings up its own server but pointing at the same
    // underlying database file. We include the runtime version in the filter
    // query to avoid a race between both tests. This feels a bit hacky.
    // Ideally, we'd run the tests sequentially or with better isolation :/.
    var now = DateTimeOffset.Now.ToUnixTimeSeconds();
    var suffix = $"{now} {System.Environment.Version} static";
    var CreateMessage = $"C# client test 0:  =?&{suffix}";

    RecordId id = await api.Create(
        new SimpleStrict(null, null, null, CreateMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    var eventStream = await api.Subscribe(id);

    var UpdatedMessage = $"C# client update test 0:  =?&{suffix}";
    await api.Update(
        id,
        new SimpleStrict(null, null, null, UpdatedMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    await api.Delete(id);

    List<Event> events = [];
    await foreach (Event msg in eventStream) {
      events.Add(msg);
    }

    Assert.Equal(2, events.Count);

    Assert.True(events[0] is UpdateEvent);
    Assert.Equal(UpdatedMessage, events[0].Value!["text_not_null"]?.ToString());

    Assert.True(events[1] is DeleteEvent);
    Assert.Equal(UpdatedMessage, events[1].Value!["text_not_null"]?.ToString());

    List<Event> tableEvents = [];
    await foreach (Event msg in tableEventStream) {
      tableEvents.Add(msg);

      if (tableEvents.Count >= 3) {
        break;
      }
    }

    Assert.Equal(3, tableEvents.Count);

    Assert.True(tableEvents[0] is InsertEvent);
    Assert.Equal(CreateMessage, tableEvents[0].Value!["text_not_null"]?.ToString());

    Assert.True(tableEvents[1] is UpdateEvent);
    Assert.Equal(UpdatedMessage, tableEvents[1].Value!["text_not_null"]?.ToString());

    Assert.True(tableEvents[2] is DeleteEvent);
    Assert.Equal(UpdatedMessage, tableEvents[2].Value!["text_not_null"]?.ToString());
  }
}
