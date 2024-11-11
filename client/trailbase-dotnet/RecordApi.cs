using System.Text.Json;

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

public class RecordApi {
  static readonly string _recordApi = "api/records/v1";

  Client client { get; }
  string name { get; }

  public RecordApi(Client client, string name) {
    this.client = client;
    this.name = name;
  }

  public async Task<T?> Read<T>(RecordId id) {
    var response = await client.Fetch<object>(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Get,
      null,
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    return JsonSerializer.Deserialize<T>(json);
  }
  public async Task<T?> Read<T>(string id) => await Read<T>(new UuidRecordId(id));
  public async Task<T?> Read<T>(long id) => await Read<T>(new IntegerRecordId(id));

  public async Task<RecordId> Create<T>(T record) {
    var response = await client.Fetch(
      $"{RecordApi._recordApi}/{name}",
      HttpMethod.Post,
      record,
      null
    );

    string json = await response.Content.ReadAsStringAsync();
    return JsonSerializer.Deserialize<ResponseRecordId>(json)!;
  }

  public async Task<List<T>> List<T>(
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

    var response = await client.Fetch<object>(
      $"{RecordApi._recordApi}/{name}",
      HttpMethod.Get,
      null,
      param
    );

    string json = await response.Content.ReadAsStringAsync();
    return JsonSerializer.Deserialize<List<T>>(json)!;
  }

  public async Task Update<T>(
    RecordId id,
    T record
  ) {
    await client.Fetch(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Patch,
      record,
      null
    );
  }

  public async Task Delete(RecordId id) {
    await client.Fetch<object>(
      $"{RecordApi._recordApi}/{name}/{id}",
      HttpMethod.Delete,
      null,
      null
    );
  }
}
