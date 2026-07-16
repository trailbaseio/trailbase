abstract class RecordId {
  @override
  String toString();

  factory RecordId.integer(int id) => _IntegerRecordId(id);
  factory RecordId.uuid(String id) => _UuidRecordId(id);
}

class _IntegerRecordId implements RecordId {
  final int id;

  const _IntegerRecordId(this.id);

  @override
  String toString() => id.toString();

  @override
  bool operator ==(Object other) {
    if (other is _IntegerRecordId) return id == other.id;
    if (other is int) return id == other;
    return false;
  }

  @override
  int get hashCode => id.hashCode;
}

extension RecordIdExtInt on int {
  RecordId id() => _IntegerRecordId(this);
}

class _UuidRecordId implements RecordId {
  final String id;

  const _UuidRecordId(this.id);

  @override
  String toString() => id;

  @override
  bool operator ==(Object other) {
    if (other is _UuidRecordId) return id == other.id;
    if (other is String) return id == other;
    return false;
  }

  @override
  int get hashCode => id.hashCode;
}

extension RecordIdExtString on String {
  RecordId id() => _UuidRecordId(this);
}
