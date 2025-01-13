using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;
using System.Net.Http.Json;
using System.Diagnostics.CodeAnalysis;

namespace TrailBase;

public class RecordId { }

public class ResponseRecordId : RecordId {
  public string id { get; }

  public ResponseRecordId(string id) {
    this.id = id;
  }

  public override string ToString() => id;
}

public class IntegerRecordId : RecordId {
  public long id { get; }

  public IntegerRecordId(long id) {
    this.id = id;
  }

  public override string ToString() => id.ToString();
}

public class UuidRecordId : RecordId {
  public Guid id { get; }

  public UuidRecordId(Guid id) {
    this.id = id;
  }

  public UuidRecordId(string id) {
    var bytes = System.Convert.FromBase64String(id.Replace('-', '+').Replace('_', '/'));
    this.id = new Guid(bytes);
  }

  public override string ToString() {
    var bytes = id.ToByteArray();
    return System.Convert.ToBase64String(bytes).Replace('+', '-').Replace('/', '_');
  }
}

public class Pagination {
  public string? cursor { get; }
  public int? limit { get; }

  public Pagination(string? cursor, int? limit) {
    this.cursor = cursor;
    this.limit = limit;
  }
}

public abstract class Event {
  public abstract JsonNode? Value { get; }

  internal static Event Parse(string message) {
    var obj = (JsonObject?)JsonNode.Parse(message);
    if (obj != null) {
      var insert = obj["Insert"];
      if (insert != null) {
        return new InsertEvent(insert);
      }

      var update = obj["Update"];
      if (update != null) {
        return new UpdateEvent(update);
      }

      var delete = obj["Delete"];
      if (delete != null) {
        return new DeleteEvent(delete);
      }

      var error = obj["Error"];
      if (error != null) {
        return new ErrorEvent(error.ToString());
      }
    }

    throw new Exception($"Failed to parse {message}");
  }
}

public class InsertEvent : Event {
  public override JsonNode? Value { get; }

  public InsertEvent(JsonNode? value) {
    this.Value = value;
  }

  public override string ToString() => $"InsertEvent({Value})";
}

public class UpdateEvent : Event {
  public override JsonNode? Value { get; }

  public UpdateEvent(JsonNode? value) {
    this.Value = value;
  }

  public override string ToString() => $"UpdateEvent({Value})";
}

public class DeleteEvent : Event {
  public override JsonNode? Value { get; }

  public DeleteEvent(JsonNode? value) {
    this.Value = value;
  }

  public override string ToString() => $"DeleteEvent({Value})";
}

public class ErrorEvent : Event {
  public override JsonNode? Value { get { return null; } }
  public string ErrorMessage { get; }

  public ErrorEvent(string errorMsg) {
    this.ErrorMessage = errorMsg;
  }

  public override string ToString() => $"ErrorEvent({ErrorMessage})";
}

[JsonSourceGenerationOptions(WriteIndented = true)]
[JsonSerializable(typeof(ResponseRecordId))]
internal partial class SerializeResponseRecordIdContext : JsonSerializerContext {
}

public class RecordApi {
  static readonly string _recordApi = "api/records/v1";
  const string DynamicCodeMessage = "Use overload with JsonTypeInfo instead";
  const string UnreferencedCodeMessage = "Use overload with JsonTypeInfo instead";

  Client client { get; }
  string name { get; }

  public RecordApi(Client client, string name) {
    this.client = client;
    this.name = name;
  }

  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(RecordId id) {
    string json = await (await ReadImpl(id)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<T>(json);
  }
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(string id) => await Read<T>(new UuidRecordId(id));
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(long id) => await Read<T>(new IntegerRecordId(id));

  public async Task<T?> Read<T>(RecordId id, JsonTypeInfo<T> jsonTypeInfo) {
    string json = await (await ReadImpl(id)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<T>(json, jsonTypeInfo);
  }
  public async Task<T?> Read<T>(string id, JsonTypeInfo<T> jsonTypeInfo) => await Read<T>(new UuidRecordId(id), jsonTypeInfo);
  public async Task<T?> Read<T>(long id, JsonTypeInfo<T> jsonTypeInfo) => await Read<T>(new IntegerRecordId(id), jsonTypeInfo);

  private async Task<HttpContent> ReadImpl(RecordId id) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Get,
      null,
      null
    );
    return response.Content;
  }

  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<RecordId> Create<T>(T record) {
    var options = new JsonSerializerOptions {
      DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };
    var recordJson = JsonContent.Create(record, typeof(T), default, options);
    return await CreateImpl(recordJson);
  }

  public async Task<RecordId> Create<T>(T record, JsonTypeInfo<T> jsonTypeInfo) {
    var recordJson = JsonContent.Create(record, jsonTypeInfo, default);
    return await CreateImpl(recordJson);
  }

  private async Task<RecordId> CreateImpl(HttpContent recordJson) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}",
      HttpMethod.Post,
      recordJson,
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    return JsonSerializer.Deserialize<ResponseRecordId>(json, SerializeResponseRecordIdContext.Default.ResponseRecordId)!;
  }

  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<List<T>> List<T>(
    Pagination? pagination,
    List<string>? order,
    List<string>? filters
  ) {
    string json = await (await ListImpl(pagination, order, filters)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<List<T>>(json) ?? [];
  }

  public async Task<List<T>> List<T>(
    Pagination? pagination,
    List<string>? order,
    List<string>? filters, JsonTypeInfo<List<T>> jsonTypeInfo
  ) {
    string json = await (await ListImpl(pagination, order, filters)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<List<T>>(json, jsonTypeInfo) ?? [];
  }

  private async Task<HttpContent> ListImpl(
    Pagination? pagination,
    List<string>? order,
    List<string>? filters
  ) {
    var param = new Dictionary<string, string>();
    if (pagination != null) {
      var cursor = pagination.cursor;
      if (cursor != null) {
        param.Add("cursor", cursor);
      }

      var limit = pagination.limit;
      if (limit != null) {
        param.Add("limit", $"{limit}");
      }
    }

    if (order != null) {
      param.Add("order", String.Join(",", order.ToArray()));
    }

    if (filters != null) {
      foreach (var filter in filters) {
        var split = filter.Split('=', 2);
        if (split.Length < 2) {
          throw new Exception($"Filter '{filter}' does not match: 'name[op]=value'");
        }
        var nameOp = split[0];
        var value = split[1];
        param.Add(nameOp, value);
      }
    }

    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}",
      HttpMethod.Get,
      null,
      param
    );

    return response.Content;
  }

  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task Update<T>(
    RecordId id,
    T record
  ) {
    var options = new JsonSerializerOptions {
      DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };
    var recordJson = JsonContent.Create(record, typeof(T), default, options);
    await UpdateImpl(id, recordJson);
  }

  public async Task Update<T>(
    RecordId id,
    T record,
    JsonTypeInfo<T> jsonTypeInfo
  ) {
    var recordJson = JsonContent.Create(record, jsonTypeInfo, default);
    await UpdateImpl(id, recordJson);
  }

  private async Task UpdateImpl(
    RecordId id,
    HttpContent recordJson
  ) {
    await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Patch,
      recordJson,
      null
    );
  }

  public async Task Delete(RecordId id) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Delete,
      null,
      null
    );
  }

  public async Task<IAsyncEnumerable<Event>> Subscribe(RecordId id) {
    var response = await SubscribeImpl(id.ToString()!);
    return StreamToEnumerableImpl(await response.ReadAsStreamAsync());
  }

  public async Task<IAsyncEnumerable<Event>> SubscribeAll() {
    var response = await SubscribeImpl("*");
    return StreamToEnumerableImpl(await response.ReadAsStreamAsync());
  }

  private async Task<HttpContent> SubscribeImpl(string id) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}/subscribe/{id}",
      HttpMethod.Get,
      null,
      null,
      HttpCompletionOption.ResponseHeadersRead
    );

    return response.Content;
  }

  private static async IAsyncEnumerable<Event> StreamToEnumerableImpl(Stream stream) {
    using (var streamReader = new StreamReader(stream)) {
      while (!streamReader.EndOfStream) {
        var message = await streamReader.ReadLineAsync();
        if (message != null) {
          message.Trim();
          if (message.StartsWith("data: ")) {
            yield return Event.Parse(message.Substring(6));
          }
        }
      }
    }
  }
}
