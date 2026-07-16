import './record_id.dart';

sealed class Operation {
  Map<String, dynamic> toJson();
}

class CreateOperation implements Operation {
  final String apiName;
  final Map<String, dynamic> value;

  const CreateOperation({
    required this.apiName,
    required this.value,
  });

  @override
  Map<String, dynamic> toJson() {
    return {
      'Create': {
        'api_name': apiName,
        'value': value,
      },
    };
  }
}

class UpdateOperation implements Operation {
  final String apiName;
  final RecordId id;
  final Map<String, dynamic> value;

  const UpdateOperation({
    required this.apiName,
    required this.id,
    required this.value,
  });

  @override
  Map<String, dynamic> toJson() {
    return {
      'Update': {
        'api_name': apiName,
        'record_id': id.toString(),
        'value': value,
      },
    };
  }
}

class DeleteOperation implements Operation {
  final String apiName;
  final RecordId id;

  const DeleteOperation({
    required this.apiName,
    required this.id,
  });

  @override
  Map<String, dynamic> toJson() {
    return {
      'Delete': {
        'api_name': apiName,
        'record_id': id.toString(),
      },
    };
  }
}
