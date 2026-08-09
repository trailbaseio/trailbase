namespace TrailBase;

/// <summary>Base for RecordId representations.</summary>
public abstract class RecordId {
  /// <summary>Serialize RecordId.</summary>
  public abstract override string ToString();

  /// <summary>Parses RecordIds.</summary>
  static public RecordId Parse(string id) {
    long value = 0;
    if (long.TryParse(id, out value)) {
      return new IntegerRecordId(value);
    }
    return new UuidRecordId(id);
  }
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
