using System.Text.Json.Nodes;

namespace TrailBase;

/// <summary>Realtime event for change subscriptions.</summary>
public abstract class Event {
  /// <summary>Get associated record value as JSON object.</summary>
  public abstract JsonNode? Value { get; }
  /// <summary>Get associated error message.</summary>
  public long? Seq { get; }

  /// <summary>InsertEvent constructor.</summary>
  public Event(long? seq) {
    this.Seq = seq;
  }

  /// <summary>Parses JSON into an Event.</summary>
  public static Event Parse(string message) {
    var obj = (JsonObject?)JsonNode.Parse(message);
    if (obj != null) {
      var seq = obj["seq"]?.GetValue<long>();

      var insert = obj["Insert"];
      if (insert != null) {
        return new InsertEvent(seq, insert);
      }

      var update = obj["Update"];
      if (update != null) {
        return new UpdateEvent(seq, update);
      }

      var delete = obj["Delete"];
      if (delete != null) {
        return new DeleteEvent(seq, delete);
      }

      var error = obj["Error"];
      if (error != null) {
        return new ErrorEvent(seq, (ErrorEvent.ErrorStatus)(error["status"]?.GetValue<int>() ?? 0), error["message"]?.GetValue<string>());
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
  public InsertEvent(long? seq, JsonNode? value) : base(seq) {
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
  public UpdateEvent(long? seq, JsonNode? value) : base(seq) {
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
  public DeleteEvent(long? seq, JsonNode? value) : base(seq) {
    this.Value = value;
  }

  /// <summary>Serialize DeleteEvent.</summary>
  public override string ToString() => $"DeleteEvent({Value})";
}

/// <summary>Error event.</summary>
public class ErrorEvent : Event {
  /// <summary>Programmatic error status.</summary>
  public enum ErrorStatus : int {
    /// Unknown error status.
    Unknown = 0,
    /// Forbidden, i.e. ACL violation.
    Forbidden = 1,
    /// Server-side event loss. Independently events can get lost between TrailBase and the client.
    Loss = 2,
  }

  /// <summary>Get associated record value as JSON object.</summary>
  public override JsonNode? Value { get { return null; } }

  /// <summary>Programmatic status of this error event.</summary>
  public ErrorStatus Status { get; }
  /// <summary>Get associated error message.</summary>
  public string? Message { get; }

  /// <summary>ErrorEvent constructor.</summary>
  public ErrorEvent(long? seq, ErrorStatus status, string? message) : base(seq) {
    this.Status = status;
    this.Message = message;
  }

  /// <summary>Serialize ErrorEvent.</summary>
  public override string ToString() => $"ErrorEvent({Status}, {Message})";
}
