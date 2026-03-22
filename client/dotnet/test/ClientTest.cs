using OtpNet;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Diagnostics.CodeAnalysis;
using Xunit;

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
[JsonSerializable(typeof(ListResponse<SimpleStrict>))]
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
    process.StartInfo.Arguments = $"run -- --data-dir ../../testfixture run -a {address} --runtime-threads 2";
    process.StartInfo.UseShellExecute = false;
    process.StartInfo.RedirectStandardOutput = true;
    process.Start();

    var client = new HttpClient();
    Task.Run(async () => {
      for (int i = 0; i < 200; ++i) {
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

  public static async Task<Client> Connect(
    string email = "admin@localhost",
    string password = "secret"
  ) {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    await client.Login(email, password);
    return client;
  }

  [Fact]
  public void IdTest() {
    var integerId = new IntegerRecordId(42);
    Assert.Equal("42", integerId.ToString());
  }

  [Fact]
  public async Task AuthTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    var mfaToken = await client.Login("admin@localhost", "secret");
    Assert.Null(mfaToken);
    var firstTokens = client.Tokens();
    Assert.NotNull(firstTokens);
    var user = client.User();
    Assert.NotNull(user);
    Assert.Equal("admin@localhost", user!.email);
    Assert.True(user!.sub != "");

    await client.Logout();

    await Task.Delay(1500);
    await client.Login("admin@localhost", "secret");

    Assert.NotEqual(client.Tokens()?.auth_token, firstTokens?.auth_token);
  }

  [Fact]
  public async Task MultiFactorAuthTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);
    var mfaToken = await client.Login("alice@trailbase.io", "secret");
    Assert.NotNull(mfaToken);

    var secret = Base32Encoding.ToBytes("YCUTAYEZ346ZUEI7FLCG57BOMZQHHRA5");
    var totp = new Totp(secret, mode: OtpHashMode.Sha1);
    var totpCode = totp.ComputeTotp();

    await client.LoginSecond(mfaToken, totpCode);
    Assert.Equal("alice@trailbase.io", client.User()?.email);
  }

  [Fact]
  public async Task OTPAuthTest() {
    var client = new Client($"http://127.0.0.1:{Constants.Port}", null);

    await client.RequestOTP("fake0@localhost");
    await client.RequestOTP("fake1@localhost", redirectUri: "/target");

    var exception = await Assert.ThrowsAsync<FetchException>(() => client.LoginOTP("fake0@localhost", "invalid"));
    Assert.Equal(System.Net.HttpStatusCode.Unauthorized, exception.Status);
  }

  [Fact]
  [RequiresDynamicCode("Testing dynamic code")]
  [RequiresUnreferencedCode("testing dynamic code")]
  public async Task RecordsTestDynamic() {
    var client = await ClientTest.Connect();
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
      var bulkIds = await api.CreateBulk([
          new SimpleStrict(null, null, null, "C# bulk create 0"),
          new SimpleStrict(null, null, null, "C# bulk create 1"),
      ]);
      Assert.Equal(2, bulkIds.Count);
    }

    {
      ListResponse<SimpleStrict> response = await api.List<SimpleStrict>(
        null,
        null,
        [new Filter(column: "text_not_null", value: $"{messages[0]}")],
        null,
        false
      )!;
      Assert.Single(response.records);
      Assert.Null(response.total_count);
      Assert.Equal(messages[0], response.records[0].text_not_null);
    }

    {
      var responseAsc = await api.List<SimpleStrict>(
        order: ["+text_not_null"],
        filters: [new Filter(column: "text_not_null", value: $"% =?&{suffix}", op: CompareOp.Like)],
        count: true
      )!;
      Assert.Equal(2, responseAsc.total_count);
      Assert.Equal(messages.Count, responseAsc.records.Count);
      Assert.Equal(messages, responseAsc.records.ConvertAll((e) => e.text_not_null));

      var responseDesc = await api.List<SimpleStrict>(
        order: ["-text_not_null"],
        filters: [new Filter("text_not_null", $"%{suffix}", op: CompareOp.Like)]
      )!;
      var recordsDesc = responseDesc.records;
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

      var response = await api.List<SimpleStrict>(filters: [new Filter("text_not_null", $"%{suffix}", op: CompareOp.Like)])!;

      Assert.Single(response.records);
    }
  }

  [Fact]
  public async Task RecordsTest() {
    var client = await ClientTest.Connect();
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
      ListResponse<SimpleStrict> response = await api.List(
        SerializeSimpleStrictContext.Default.ListResponseSimpleStrict,
        filters: [new Filter("text_not_null", $"{messages[0]}")]
      )!;
      Assert.Single(response.records);
      Assert.Equal(messages[0], response.records[0].text_not_null);
    }

    {
      var responseAsc = await api.List(
        SerializeSimpleStrictContext.Default.ListResponseSimpleStrict,
        order: ["+text_not_null"],
        filters: [new Filter("text_not_null", $"% =?&{suffix}", op: CompareOp.Like)]
      )!;
      var recordsAsc = responseAsc.records;
      Assert.Equal(messages.Count, recordsAsc.Count);
      Assert.Equal(messages, recordsAsc.ConvertAll((e) => e.text_not_null));

      var responseDesc = await api.List(
        SerializeSimpleStrictContext.Default.ListResponseSimpleStrict,
        order: ["-text_not_null"],
        filters: [new Filter("text_not_null", $"%{suffix}", op: CompareOp.Like)]
      )!;
      var recordsDesc = responseDesc.records;
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

      var response = await api.List(
        SerializeSimpleStrictContext.Default.ListResponseSimpleStrict,
        filters: [new Filter("text_not_null", $"%{suffix}", op: CompareOp.Like)]
      )!;

      Assert.Single(response.records);
    }
  }

  [Fact]
  [RequiresDynamicCode("Testing dynamic code")]
  [RequiresUnreferencedCode("testing dynamic code")]
  public async Task ExpandForeignRecordsTest() {
    var client = await ClientTest.Connect();
    var api = client.Records("comment");

    {
      var comment = await api.Read<JsonObject>(1)!;
      Assert.NotNull(comment);
      Assert.Equal(1, comment["id"]!.GetValue<int>());
      Assert.Equal("first comment", comment["body"]!.GetValue<string>());
      Assert.NotNull(comment["author"]!["id"]);
      Assert.Null(comment["author"]!["data"]);
      Assert.NotNull(comment["post"]!["id"]);
      Assert.Null(comment["post"]!["data"]);
    }

    {
      var comment = await api.Read<JsonObject>(1, expand: ["post"])!;
      Assert.NotNull(comment);
      Assert.Equal(1, comment["id"]!.GetValue<int>());
      Assert.Null(comment["author"]!["data"]);
      Assert.Equal("first post", comment["post"]!["data"]!["title"]!.GetValue<string>());
    }

    {
      var response = await api.List<JsonObject>(
        pagination: new Pagination(limit: 2),
        expand: ["author", "post"],
        order: ["-id"]
      );

      Assert.Equal(2, response.records.Count);
      var first = response.records[0];

      Assert.Equal(2, first["id"]!.GetValue<int>());
      Assert.Equal("second comment", first["body"]!.GetValue<string>());
      Assert.Equal("SecondUser", first["author"]!["data"]!["name"]!.GetValue<string>());
      Assert.Equal("first post", first["post"]!["data"]!["title"]!.GetValue<string>());

      var second = response.records[1];

      var offsetResponse = await api.List<JsonObject>(
        pagination: new Pagination(limit: 1, offset: 1),
        expand: ["author", "post"],
        order: ["-id"]
      );

      Assert.Single(offsetResponse.records);
      Assert.True(JsonObject.DeepEquals(second, offsetResponse.records[0]));
    }
  }

  [Fact]
  public async Task RealtimeTest() {
    var client = await ClientTest.Connect();
    var api = client.Records("simple_strict_table");

    var tableEventStream = await api.SubscribeAll();

    // Dotnet runs tests for multiple target framework versions in parallel.
    // Each test currently brings up its own server but pointing at the same
    // underlying database file. We include the runtime version in the filter
    // query to avoid a race between both tests. This feels a bit hacky.
    // Ideally, we'd run the tests sequentially or with better isolation :/.
    var suffix = $"{DateTimeOffset.Now.ToUnixTimeSeconds()} {System.Environment.Version} static";

    var createMessage = $"C# client realtime test 0:  =?&{suffix}";
    RecordId id = await api.Create(
        new SimpleStrict(null, null, null, createMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    var eventStream = await api.Subscribe(id);

    var updatedMessage = $"C# client realtime update test 0:  =?&{suffix}";
    await api.Update(
        id,
        new SimpleStrict(null, null, null, updatedMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    await api.Delete(id);

    {
      // Record events.
      List<Event> events = [];
      await foreach (Event msg in eventStream) {
        events.Add(msg);
      }

      Assert.Equal(2, events.Count);

      var ev0 = events[0] as UpdateEvent;
      Assert.NotNull(ev0);
      Assert.Equal((uint)1, ev0.Seq);
      Assert.Equal(updatedMessage, ev0.Value!["text_not_null"]?.ToString());

      var ev1 = events[1] as DeleteEvent;
      Assert.NotNull(ev1);
      Assert.Equal((uint)2, ev1.Seq);
      Assert.Equal(updatedMessage, ev1.Value!["text_not_null"]?.ToString());
    }

    {
      // Table events.
      List<Event> tableEvents = [];
      await foreach (Event msg in tableEventStream) {
        tableEvents.Add(msg);

        // TODO: Maybe use a timeout instead.
        if (tableEvents.Count >= 3) {
          break;
        }
      }

      Assert.Equal(3, tableEvents.Count);

      var ev0 = tableEvents[0] as InsertEvent;
      Assert.NotNull(ev0);
      Assert.Equal((uint)1, ev0.Seq);
      Assert.Equal(createMessage, ev0.Value!["text_not_null"]?.ToString());

      var ev1 = tableEvents[1] as UpdateEvent;
      Assert.NotNull(ev1);
      Assert.Equal(updatedMessage, ev1.Value!["text_not_null"]?.ToString());

      var ev2 = tableEvents[2] as DeleteEvent;
      Assert.NotNull(ev2);
      Assert.Equal(updatedMessage, ev2.Value!["text_not_null"]?.ToString());
    }
  }

  [Fact]
  public async Task RealtimeTableSubscriptionWithFilterTest() {
    var client = await ClientTest.Connect();
    var api = client.Records("simple_strict_table");

    // Dotnet runs tests for multiple target framework versions in parallel.
    // Each test currently brings up its own server but pointing at the same
    // underlying database file. We include the runtime version in the filter
    // query to avoid a race between both tests. This feels a bit hacky.
    // Ideally, we'd run the tests sequentially or with better isolation :/.
    var suffix = $"{DateTimeOffset.Now.ToUnixTimeSeconds()} {System.Environment.Version} static";
    var updatedMessage = $"C# client updated realtime test 42: {suffix}";

    var tableEventStream = await api.SubscribeAll(filters: [new Filter(column: "text_not_null", value: updatedMessage)]);

    var createMessage = $"C# client realtime test 42:  =?&{suffix}";
    RecordId id = await api.Create(
        new SimpleStrict(null, null, null, createMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    var eventStream = await api.Subscribe(id);

    await api.Update(
        id,
        new SimpleStrict(null, null, null, updatedMessage),
        SerializeSimpleStrictContext.Default.SimpleStrict
    );

    await api.Delete(id);

    List<Event> events = [];
    await foreach (Event msg in tableEventStream) {
      events.Add(msg);

      // TODO: Maybe use a timeout instead.
      if (events.Count >= 2) {
        break;
      }
    }

    Assert.Equal(2, events.Count);

    Assert.True(events[0] is UpdateEvent);
    Assert.Equal(updatedMessage, events[0].Value!["text_not_null"]?.ToString());

    Assert.True(events[1] is DeleteEvent);
    Assert.Equal(updatedMessage, events[1].Value!["text_not_null"]?.ToString());
  }
}
