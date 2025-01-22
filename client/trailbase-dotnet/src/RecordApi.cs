using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;
using System.Net.Http.Json;
using System.Diagnostics.CodeAnalysis;

namespace TrailBase;

/// <summary>Base for RecordId representations.</summary>
public abstract class RecordId {
  /// <summary>Serialize RecordId.</summary>
  public abstract override string ToString();
}

/// <summary>Un-typed record id.</summary>
public class ResponseRecordId : RecordId {
  /// <summary>Serialized id, could be integer or UUID.</summary>
  public string id { get; }

  /// <summary>ResponseRecordId constructor.</summary>
  public ResponseRecordId(string id) {
    this.id = id;
  }

  /// <summary>Serialize RecordId.</summary>
  public override string ToString() => id;
}

/// <summary>Integer record id.</summary>
public class IntegerRecordId : RecordId {
  long id { get; }

  /// <summary>Integer record id constructor.</summary>
  public IntegerRecordId(long id) {
    this.id = id;
  }

  /// <summary>Serialize RecordId.</summary>
  public override string ToString() => id.ToString();
}

/// <summary>UUID record id.</summary>
public class UuidRecordId : RecordId {
  Guid id { get; }

  /// <summary>UUID record id constructor.</summary>
  public UuidRecordId(Guid id) {
    this.id = id;
  }

  /// <summary>UUID record id constructor.</summary>
  public UuidRecordId(string id) {
    var bytes = System.Convert.FromBase64String(id.Replace('-', '+').Replace('_', '/'));
    this.id = new Guid(bytes);
  }

  /// <summary>Serialize UuidRecordId.</summary>
  public override string ToString() {
    var bytes = id.ToByteArray();
    return System.Convert.ToBase64String(bytes).Replace('+', '-').Replace('/', '_');
  }
}

/// <summary>Pagination state representation.</summary>
public class Pagination {
  /// <summary>Limit of elements per page.</summary>
  public int? limit { get; }
  /// <summary>Offset cursor.</summary>
  public string? cursor { get; }

  /// <summary>Pagination constructor.</summary>
  public Pagination(int? limit = null, string? cursor = null) {
    this.cursor = cursor;
    this.limit = limit;
  }
}

/// <summary>
/// Representation of ListResponse JSON objects.
/// </summary>
// @JsonSerializable(explicitToJson: true)
public class ListResponse<T> {
  /// <summary>List cursor for subsequent fetches.</summary>
  public string? cursor { get; }
  /// <summary>The actual records.</summary>
  public List<T> records { get; }

  /// <summary>ListResponse constructor.</summary>
  [JsonConstructor]
  public ListResponse(
      string? cursor,
      List<T>? records
  ) {
    this.cursor = cursor;
    this.records = records ?? [];
  }
}

/// <summary>Realtime event for change subscriptions.</summary>
public abstract class Event {
  /// <summary>Get associated record value as JSON object.</summary>
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

/// <summary>Record insertion event.</summary>
public class InsertEvent : Event {
  /// <summary>Get associated record value as JSON object.</summary>
  public override JsonNode? Value { get; }

  /// <summary>InsertEvent constructor.</summary>
  public InsertEvent(JsonNode? value) {
    this.Value = value;
  }

  /// <summary>Serialize InsertEvent.</summary>
  public override string ToString() => $"InsertEvent({Value})";
}

/// <summary>Record update event.</summary>
public class UpdateEvent : Event {
  /// <summary>Get associated record value as JSON object.</summary>
  public override JsonNode? Value { get; }

  /// <summary>UpdateEvent constructor.</summary>
  public UpdateEvent(JsonNode? value) {
    this.Value = value;
  }

  /// <summary>Serialize UpdateEvent.</summary>
  public override string ToString() => $"UpdateEvent({Value})";
}

/// <summary>Record deletion event.</summary>
public class DeleteEvent : Event {
  /// <summary>Get associated record value as JSON object.</summary>
  public override JsonNode? Value { get; }

  /// <summary>DeleteEvent constructor.</summary>
  public DeleteEvent(JsonNode? value) {
    this.Value = value;
  }

  /// <summary>Serialize DeleteEvent.</summary>
  public override string ToString() => $"DeleteEvent({Value})";
}

/// <summary>Error event.</summary>
public class ErrorEvent : Event {
  /// <summary>Get associated record value as JSON object.</summary>
  public override JsonNode? Value { get { return null; } }
  /// <summary>Get associated error message.</summary>
  public string ErrorMessage { get; }

  /// <summary>ErrorEvent constructor.</summary>
  public ErrorEvent(string errorMsg) {
    this.ErrorMessage = errorMsg;
  }

  /// <summary>Serialize ErrorEvent.</summary>
  public override string ToString() => $"ErrorEvent({ErrorMessage})";
}

[JsonSourceGenerationOptions(WriteIndented = true)]
[JsonSerializable(typeof(ResponseRecordId))]
[JsonSerializable(typeof(ListResponse<JsonObject>))]
internal partial class SerializeResponseRecordIdContext : JsonSerializerContext {
}

/// <summary>Main API to interact with Records.</summary>
public class RecordApi {
  static readonly string _recordApi = "api/records/v1";
  const string DynamicCodeMessage = "Use overload with JsonTypeInfo instead";
  const string UnreferencedCodeMessage = "Use overload with JsonTypeInfo instead";

  Client client { get; }
  string name { get; }

  internal RecordApi(Client client, string name) {
    this.client = client;
    this.name = name;
  }

  /// <summary>Read the record with given id.</summary>
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(RecordId id) {
    string json = await (await ReadImpl(id)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<T>(json);
  }
  /// <summary>Read the record with given id.</summary>
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(string id) => await Read<T>(new UuidRecordId(id));
  /// <summary>Read the record with given id.</summary>
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<T?> Read<T>(long id) => await Read<T>(new IntegerRecordId(id));

  /// <summary>Read the record with given id.</summary>
  public async Task<T?> Read<T>(RecordId id, JsonTypeInfo<T> jsonTypeInfo) {
    string json = await (await ReadImpl(id)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<T>(json, jsonTypeInfo);
  }
  /// <summary>Read the record with given id.</summary>
  public async Task<T?> Read<T>(string id, JsonTypeInfo<T> jsonTypeInfo)
    => await Read<T>(new UuidRecordId(id), jsonTypeInfo);
  /// <summary>Read the record with given id.</summary>
  public async Task<T?> Read<T>(long id, JsonTypeInfo<T> jsonTypeInfo)
    => await Read<T>(new IntegerRecordId(id), jsonTypeInfo);

  private async Task<HttpContent> ReadImpl(RecordId id) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Get,
      null,
      null
    );
    return response.Content;
  }

  /// <summary>Create a new record with the given value.</summary>
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<RecordId> Create<T>(T record) {
    var options = new JsonSerializerOptions {
      DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull
    };
    var recordJson = JsonContent.Create(record, typeof(T), default, options);
    return await CreateImpl(recordJson);
  }

  /// <summary>Create a new record with the given value.</summary>
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
    return JsonSerializer.Deserialize<ResponseRecordId>(
        json,
        SerializeResponseRecordIdContext.Default.ResponseRecordId
    )!;
  }

  /// <summary>
  /// List records.
  /// </summary>
  /// <param name="pagination">Pagination state.</param>
  /// <param name="order">Sort results by the given columns in ascending/descending order, e.g. "-col_name".</param>
  /// <param name="filters">Results filters, e.g. "col0[gte]=100".</param>
  [RequiresDynamicCode(DynamicCodeMessage)]
  [RequiresUnreferencedCode(UnreferencedCodeMessage)]
  public async Task<ListResponse<T>> List<T>(
    Pagination? pagination = null,
    List<string>? order = null,
    List<string>? filters = null
  ) {
    string json = await (await ListImpl(pagination, order, filters)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<ListResponse<T>>(json) ?? new ListResponse<T>(null, []);
  }

  /// <summary>
  /// List records.
  /// </summary>
  /// <param name="jsonTypeInfo">Serialization type info for AOT mode.</param>
  /// <param name="pagination">Pagination state.</param>
  /// <param name="order">Sort results by the given columns in ascending/descending order, e.g. "-col_name".</param>
  /// <param name="filters">Results filters, e.g. "col0[gte]=100".</param>
  public async Task<ListResponse<T>> List<T>(
    JsonTypeInfo<ListResponse<T>> jsonTypeInfo,
    Pagination? pagination = null,
    List<string>? order = null,
    List<string>? filters = null
  ) {
    string json = await (await ListImpl(pagination, order, filters)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<ListResponse<T>>(json, jsonTypeInfo) ?? new ListResponse<T>(null, []);
  }

  /// <summary>
  /// List records.
  /// </summary>
  /// <param name="pagination">Pagination state.</param>
  /// <param name="order">Sort results by the given columns in ascending/descending order, e.g. "-col_name".</param>
  /// <param name="filters">Results filters, e.g. "col0[gte]=100".</param>
  public async Task<ListResponse<JsonObject>> List(
    Pagination? pagination = null,
    List<string>? order = null,
    List<string>? filters = null
  ) {
    string json = await (await ListImpl(pagination, order, filters)).ReadAsStringAsync();
    return JsonSerializer.Deserialize<ListResponse<JsonObject>>(
        json, SerializeResponseRecordIdContext.Default.ListResponseJsonObject) ?? new ListResponse<JsonObject>(null, []);
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

  /// <summary>Update record with the given id with the given values.</summary>
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

  /// <summary>Update record with the given id with the given values.</summary>
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

  /// <summary>Delete record with the given id.</summary>
  public async Task Delete(RecordId id) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Delete,
      null,
      null
    );
  }

  /// <summary>Listen for changes to record with given id.</summary>
  public async Task<IAsyncEnumerable<Event>> Subscribe(RecordId id) {
    var response = await SubscribeImpl(id.ToString()!);
    return StreamToEnumerableImpl(await response.ReadAsStreamAsync());
  }

  /// <summary>Listen for all accessible changes to this Record API.</summary>
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
