import 'package:trailbase/src/client.dart';

class Operation {
  CreateOperation? create;
  UpdateOperation? update;
  DeleteOperation? delete;

  Operation({this.create, this.update, this.delete});

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    if (create != null) {
      data['Create'] = create!.toJson();
    }
    if (update != null) {
      data['Update'] = update!.toJson();
    }
    if (delete != null) {
      data['Delete'] = delete!.toJson();
    }
    return data;
  }
}

class CreateOperation {
  String apiName;
  Map<String, dynamic> value;

  CreateOperation({required this.apiName, required this.value});

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    data['api_name'] = apiName;
    data['value'] = value;
    return data;
  }
}

class UpdateOperation {
  String apiName;
  RecordId recordId;
  Map<String, dynamic> value;

  UpdateOperation({
    required this.apiName,
    required this.recordId,
    required this.value,
  });

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    data['api_name'] = apiName;
    data['record_id'] = recordId.toString();
    data['value'] = value;
    return data;
  }
}

class DeleteOperation {
  String apiName;
  RecordId recordId;

  DeleteOperation({required this.apiName, required this.recordId});

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    data['api_name'] = apiName;
    data['record_id'] = recordId.toString();
    return data;
  }
}

class TransactionRequest {
  List<Operation> operations;

  TransactionRequest({required this.operations});

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    data['operations'] = operations.map((e) => e.toJson()).toList();
    return data;
  }
}

abstract class ITransactionBatch {
  IApiBatch api(String apiName);
  Future<List<RecordId>> send();
}

abstract class IApiBatch {
  ITransactionBatch create(Map<String, dynamic> value);
  ITransactionBatch update(RecordId recordId, Map<String, dynamic> value);
  ITransactionBatch delete(RecordId recordId);
}

class TransactionBatch implements ITransactionBatch {
  final Client _client;
  final List<Operation> _operations = [];

  TransactionBatch(this._client);

  @override
  IApiBatch api(String apiName) {
    return ApiBatch(this, apiName);
  }

  @override
  Future<List<RecordId>> send() async {
    final request = TransactionRequest(operations: _operations);
    final response = await _client.fetch(
      'api/transaction/v1/execute',
      method: 'POST',
      data: request.toJson(),
    );

    if ((response.statusCode ?? 400) > 200) {
      throw Exception('${response.data} ${response.statusMessage}');
    }

    final result = ResponseRecordIds.fromJson(response.data);
    return result.toRecordIds();
  }

  void addOperation(Operation operation) {
    _operations.add(operation);
  }
}

class ApiBatch implements IApiBatch {
  final TransactionBatch _batch;
  final String _apiName;

  ApiBatch(this._batch, this._apiName);

  @override
  ITransactionBatch create(Map<String, dynamic> value) {
    _batch.addOperation(
      Operation(create: CreateOperation(apiName: _apiName, value: value)),
    );
    return _batch;
  }

  @override
  ITransactionBatch update(RecordId recordId, Map<String, dynamic> value) {
    _batch.addOperation(
      Operation(
        update: UpdateOperation(
          apiName: _apiName,
          recordId: recordId,
          value: value,
        ),
      ),
    );
    return _batch;
  }

  @override
  ITransactionBatch delete(RecordId recordId) {
    _batch.addOperation(
      Operation(delete: DeleteOperation(apiName: _apiName, recordId: recordId)),
    );
    return _batch;
  }
}
