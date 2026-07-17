using System.Text.Json;
using System.Text.Json.Nodes;
using System.Text.Json.Serialization;
using System.Text.Json.Serialization.Metadata;
using System.Net.Http.Json;
using System.Diagnostics.CodeAnalysis;

namespace TrailBase;

/// <summary>Operation for batch processing and transactions.</summary>
public abstract class Operation {
  /// <summary>Serialize to Json.</summary>
  public abstract JsonNode ToJson();
}

/// <summary>CreateOperation for batch processing and transactions.</summary>
sealed public class CreateOperation : Operation {
  /// <summary>Name of the target RecordApi.</summary>
  public string api_name { get; }
  /// <summary>Get associated record value as JSON object.</summary>
  public JsonNode value { get; }

  /// <summary>CreateOperation constructor.</summary>
  public CreateOperation(string apiName, JsonNode value) {
    this.api_name = apiName;
    this.value = value;
  }

  /// <summary>Serialize to Json.</summary>
  public override JsonNode ToJson() {
    var node = new JsonObject();
    node.Add("Create", JsonSerializer.SerializeToNode(this, SerializeOperationsIdContext.Default.CreateOperation));
    return node;
  }
}

/// <summary>UpdateOperation for batch processing and transactions.</summary>
sealed public class UpdateOperation : Operation {
  /// <summary>Name of the target RecordApi.</summary>
  public string api_name { get; }
  /// <summary>Id of the target record.</summary>
  public string record_id { get; }
  /// <summary>Get associated record value as JSON object.</summary>
  public JsonNode value { get; }

  /// <summary>UpdateOperation constructor.</summary>
  public UpdateOperation(string apiName, string id, JsonNode value) {
    this.api_name = apiName;
    this.record_id = id;
    this.value = value;
  }

  /// <summary>Serialize to Json.</summary>
  public override JsonNode ToJson() {
    var node = new JsonObject();
    node.Add("Update", JsonSerializer.SerializeToNode(this, SerializeOperationsIdContext.Default.UpdateOperation));
    return node;
  }
}

/// <summary>DeleteOperation for batch processing and transactions.</summary>
sealed public class DeleteOperation : Operation {
  /// <summary>Name of the target RecordApi.</summary>
  public string api_name { get; }
  /// <summary>Id of the target record.</summary>
  public string record_id { get; }

  /// <summary>DeleteOperation constructor.</summary>
  public DeleteOperation(string apiName, string id) {
    this.api_name = apiName;
    this.record_id = id;
  }

  /// <summary>Serialize to Json.</summary>
  public override JsonNode ToJson() {
    var node = new JsonObject();
    node.Add("Delete", JsonSerializer.SerializeToNode(this, SerializeOperationsIdContext.Default.DeleteOperation));
    return node;
  }
}

/// <summary>OperationsRequest for batch processing and transactions.</summary>
public class OperationsRequest {
  /// <summary>List of operations.</summary>
  public List<Operation> operations { get; }
  /// <summary>Whether this batch should be processed as a transaction.</summary>
  public bool transaction { get; }

  /// <summary>OperationsRequest constructor.</summary>
  public OperationsRequest(List<Operation> operations, bool transaction) {
    this.operations = operations;
    this.transaction = transaction;
  }

  /// <summary>Serialize to Json.</summary>
  public JsonNode ToJson() {
    var node = new JsonObject();
    node.Add("operations", new JsonArray(operations.Select(o => o.ToJson()).ToArray()));
    node.Add("transaction", transaction);
    return node;
  }
}

/// <summary>OperationResult for batch processing and transactions.</summary>
public abstract class OperationResult { }

/// <summary>OperationIdResult for batch processing and transactions.</summary>
sealed public class OperationIdResult : OperationResult {
  /// <summary>Id of the target record.</summary>
  public string id { get; }

  /// <summary>OperationIdResult constructor.</summary>
  public OperationIdResult(string id) {
    this.id = id;
  }
}

/// <summary>OperationErrorResult for batch processing and transactions.</summary>
sealed public class OperationErrorResult : OperationResult {
  /// <summary>Error of operation.</summary>
  public string error { get; }

  /// <summary>OperationErrorResult constructor.</summary>
  public OperationErrorResult(string error) {
    this.error = error;
  }
}

/// <summary>OperationResponse for batch processing and transactions.</summary>
public class OperationsResponse {
  /// <summary>Results from processing operations.</summary>
  public List<OperationResult> results { get; }

  /// <summary>OperationsResponse constructor.</summary>
  public OperationsResponse(List<OperationResult> results) {
    this.results = results;
  }

  /// <summary>Parses JSON into an Event.</summary>
  public static OperationsResponse Parse(string message) {
    var obj = (JsonObject?)JsonNode.Parse(message);
    if (obj != null) {
      var results = (JsonArray?)obj["results"];
      if (results != null) {
        List<OperationResult> r = [];

        foreach (var result in results) {
          var id = result?["Id"];
          if (id != null) {
            r.Add(new OperationIdResult(id.GetValue<string>()));
            continue;
          }

          var err = result?["Error"];
          if (err != null) {
            r.Add(new OperationErrorResult(err.GetValue<string>()));
            continue;
          }

          throw new Exception($"Failed to parse {message}");
        }

        return new OperationsResponse(r);
      }
    }

    throw new Exception($"Failed to parse {message}");
  }
}

[JsonSourceGenerationOptions(WriteIndented = true)]
[JsonSerializable(typeof(CreateOperation))]
[JsonSerializable(typeof(UpdateOperation))]
[JsonSerializable(typeof(DeleteOperation))]
internal partial class SerializeOperationsIdContext : JsonSerializerContext {
}
