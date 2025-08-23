using System;
using System.Collections.Generic;
using System.Diagnostics.CodeAnalysis;
using System.Net.Http;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;
using System.Threading.Tasks;

namespace TrailBase;

[JsonConverter(typeof(OperationJsonConverter))]
internal abstract class Operation {
  [JsonPropertyName("api_name")]
  public string ApiName { get; set; } = string.Empty;

  public static Operation Create(string apiName, JsonObject value)
      => new CreateOperation { ApiName = apiName, Value = value };

  public static Operation Update(string apiName, string recordId, JsonObject value)
      => new UpdateOperation { ApiName = apiName, RecordId = recordId, Value = value };

  public static Operation Delete(string apiName, string recordId)
      => new DeleteOperation { ApiName = apiName, RecordId = recordId };
}

internal class CreateOperation : Operation {
  [JsonPropertyName("value")]
  public JsonObject Value { get; set; } = new();
}

internal class UpdateOperation : Operation {
  [JsonPropertyName("record_id")]
  public string RecordId { get; set; } = string.Empty;

  [JsonPropertyName("value")]
  public JsonObject Value { get; set; } = new();
}

internal class DeleteOperation : Operation {
  [JsonPropertyName("record_id")]
  public string RecordId { get; set; } = string.Empty;
}

[RequiresDynamicCode("JSON serialization may require dynamic code")]
[RequiresUnreferencedCode("JSON serialization may require unreferenced code")]
internal class OperationJsonConverter : JsonConverter<Operation> {
  public override Operation? Read(ref Utf8JsonReader reader, Type typeToConvert, JsonSerializerOptions options) {
    using (var doc = JsonDocument.ParseValue(ref reader)) {
      var root = doc.RootElement;
      if (root.TryGetProperty("Create", out var createElem)) { return JsonSerializer.Deserialize<CreateOperation>(createElem.GetRawText(), options); }
      if (root.TryGetProperty("Update", out var updateElem)) { return JsonSerializer.Deserialize<UpdateOperation>(updateElem.GetRawText(), options); }
      if (root.TryGetProperty("Delete", out var deleteElem)) { return JsonSerializer.Deserialize<DeleteOperation>(deleteElem.GetRawText(), options); }
      throw new JsonException("Unknown operation type");
    }
  }

  public override void Write(Utf8JsonWriter writer, Operation value, JsonSerializerOptions options) {
    writer.WriteStartObject();

    switch (value) {
      case CreateOperation create:
        writer.WritePropertyName("Create");
        JsonSerializer.Serialize(writer, create, typeof(CreateOperation), options);
        break;
      case UpdateOperation update:
        writer.WritePropertyName("Update");
        JsonSerializer.Serialize(writer, update, typeof(UpdateOperation), options);
        break;
      case DeleteOperation delete:
        writer.WritePropertyName("Delete");
        JsonSerializer.Serialize(writer, delete, typeof(DeleteOperation), options);
        break;
      default:
        throw new NotSupportedException($"Operation of type {value.GetType()} is not supported.");
    }

    writer.WriteEndObject();
  }
}

internal class TransactionRequest {
  [JsonPropertyName("operations")]
  public List<Operation> Operations { get; set; } = new();
}

internal class TransactionResponse {
  [JsonPropertyName("ids")]
  public List<string> Ids { get; set; } = new();
}

/// <summary>Transaction</summary>
public interface ITransactionBatch {
  /// <summary>Api</summary>
  IApiBatch Api(string apiName);

  /// <summary>Send</summary>
  [RequiresDynamicCode("JSON serialization may require dynamic code")]
  [RequiresUnreferencedCode("JSON serialization may require unreferenced code")]
  Task<List<string>> Send();
}

/// <summary>Api</summary>
public interface IApiBatch {
  /// <summary>Create</summary>
  ITransactionBatch Create<T>(T record, JsonTypeInfo<T> jsonTypeInfo);
  /// <summary>Update</summary>
  ITransactionBatch Update<T>(RecordId recordId, T record, JsonTypeInfo<T> jsonTypeInfo);
  /// <summary>Delete</summary>
  ITransactionBatch Delete(RecordId recordId);
}

/// <summary>New transaction batch.</summary>
public class TransactionBatch : ITransactionBatch {
  static readonly string _transactionApi = "api/transaction/v1/execute";
  private readonly Client _client;
  private readonly List<Operation> _operations = new();

  /// <inheritdoc/>
  public TransactionBatch(Client client) {
    _client = client;
  }

  /// <summary>Api.</summary>
  public IApiBatch Api(string apiName) {
    return new ApiBatch(this, apiName);
  }

  /// <summary>Send transaction batch.</summary>
  [RequiresDynamicCode("JSON serialization may require dynamic code")]
  [RequiresUnreferencedCode("JSON serialization may require unreferenced code")]
  public async Task<List<string>> Send() {
    var request = new TransactionRequest { Operations = _operations };
    var response = await _client.Fetch(
        TransactionBatch._transactionApi,
        HttpMethod.Post,
        JsonContent.Create(request),
        null
    );

    string json = await response.Content.ReadAsStringAsync();
    var result = JsonSerializer.Deserialize<TransactionResponse>(json);

    return result?.Ids ?? new List<string>();
  }

  internal void AddOperation(Operation operation) {
    _operations.Add(operation);
  }
}

internal class ApiBatch : IApiBatch {
  private readonly TransactionBatch _batch;
  private readonly string _apiName;

  public ApiBatch(TransactionBatch batch, string apiName) {
    _batch = batch;
    _apiName = apiName;
  }

  public ITransactionBatch Create<T>(T record, JsonTypeInfo<T> jsonTypeInfo) {
    var value = ToJsonObject(record, jsonTypeInfo);
    _batch.AddOperation(Operation.Create(_apiName, value));
    return _batch;
  }

  public ITransactionBatch Update<T>(RecordId recordId, T record, JsonTypeInfo<T> jsonTypeInfo) {
    var value = ToJsonObject(record, jsonTypeInfo);
    _batch.AddOperation(Operation.Update(_apiName, recordId.ToString(), value));
    return _batch;
  }

  public ITransactionBatch Delete(RecordId recordId) {
    _batch.AddOperation(Operation.Delete(_apiName, recordId.ToString()));
    return _batch;
  }

  private static JsonObject ToJsonObject<T>(T record, JsonTypeInfo<T> jsonTypeInfo) {
    var node = JsonSerializer.SerializeToNode(record, jsonTypeInfo);

    return node as JsonObject ?? throw new InvalidOperationException("The provided record did not serialize to a JSON object.");
  }
}
