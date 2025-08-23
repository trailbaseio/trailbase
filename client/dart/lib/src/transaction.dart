import 'package:trailbase/src/client.dart';

class Operation {
  _CreateOperation? create;
  _UpdateOperation? update;
  _DeleteOperation? delete;

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

class _CreateOperation {
  String apiName;
  Map<String, dynamic> value;

  _CreateOperation({required this.apiName, required this.value});

  Map<String, dynamic> toJson() {
    final Map<String, dynamic> data = {};
    data['api_name'] = apiName;
    data['value'] = value;
    return data;
  }
}

class _UpdateOperation {
  String apiName;
  RecordId recordId;
  Map<String, dynamic> value;

  _UpdateOperation({
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

class _DeleteOperation {
  String apiName;
  RecordId recordId;

  _DeleteOperation({required this.apiName, required this.recordId});

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
  static const String _transactionApi = 'api/transaction/v1/execute';

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
      TransactionBatch._transactionApi,
      method: 'POST',
      data: request.toJson(),
    );

    if ((response.statusCode ?? 400) > 200) {
      throw Exception('${response.data} ${response.statusMessage}');
    }

    final result = ResponseRecordIds.fromJson(response.data);
    return result.toRecordIds();
  }

  void _addOperation(Operation operation) {
    _operations.add(operation);
  }
}

class ApiBatch implements IApiBatch {
  final TransactionBatch _batch;
  final String _apiName;

  ApiBatch(this._batch, this._apiName);

  @override
  ITransactionBatch create(Map<String, dynamic> value) {
    _batch._addOperation(
      Operation(create: _CreateOperation(apiName: _apiName, value: value)),
    );
    return _batch;
  }

  @override
  ITransactionBatch update(RecordId recordId, Map<String, dynamic> value) {
    _batch._addOperation(
      Operation(
        update: _UpdateOperation(
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
    _batch._addOperation(
      Operation(
          delete: _DeleteOperation(apiName: _apiName, recordId: recordId)),
    );
    return _batch;
  }
}
