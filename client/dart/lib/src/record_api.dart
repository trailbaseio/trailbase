import 'dart:async';
import 'dart:convert';

import './client.dart';
import './operations.dart';
import './record_id.dart';
import './sse.dart';
import './transport.dart';

class Pagination {
  final String? cursor;
  final int? limit;
  final int? offset;

  const Pagination({
    this.cursor,
    this.limit,
    this.offset,
  });
}

class ListResponse {
  final String? cursor;
  final List<Map<String, dynamic>> records;
  final int? totalCount;

  const ListResponse({
    this.cursor,
    required this.records,
    this.totalCount,
  });

  ListResponse.fromJson(Map<String, dynamic> json)
      : cursor = json['cursor'],
        records = (json['records'] as List).cast<Map<String, dynamic>>(),
        totalCount = json['total_count'];
}

enum CompareOp {
  equal,
  notEqual,
  lessThan,
  lessThanEqual,
  greaterThan,
  greaterThanEqual,
  like,
  regexp,
  stWithin,
  stIntersects,
  stContains,
  isNull,
  isNotNull,
}

sealed class FilterBase {
  const FilterBase();
}

class Filter extends FilterBase {
  final String column;
  final CompareOp? op;
  final String value;

  const Filter({
    required this.column,
    required this.value,
    this.op,
  });

  /// Filter rows where `column` IS NULL.
  ///
  /// Wire format: `filter[<column>][$is]=NULL`.
  const Filter.isNull({required String column})
      : this(column: column, value: 'NULL', op: CompareOp.isNull);

  /// Filter rows where `column` IS NOT NULL.
  ///
  /// Wire format: `filter[<column>][$is]=!NULL`.
  const Filter.isNotNull({required String column})
      : this(column: column, value: '!NULL', op: CompareOp.isNotNull);
}

class And extends FilterBase {
  final List<FilterBase> filters;

  const And(this.filters);
}

class Or extends FilterBase {
  final List<FilterBase> filters;

  const Or(this.filters);
}

class RecordApi {
  final String _name;
  final Client _client;

  const RecordApi(this._client, this._name);

  Future<ListResponse> list({
    Pagination? pagination,
    List<String>? order,
    List<FilterBase>? filters,
    bool? count,
    List<String>? expand,
  }) async {
    final params = <String, String>{};
    if (pagination != null) {
      final cursor = pagination.cursor;
      if (cursor != null) params['cursor'] = cursor;

      final limit = pagination.limit;
      if (limit != null) params['limit'] = limit.toString();

      final offset = pagination.offset;
      if (offset != null) params['offset'] = offset.toString();
    }

    if (order != null) params['order'] = order.join(',');
    if (count ?? false) params['count'] = 'true';
    if (expand != null) params['expand'] = expand.join(',');

    for (final filter in filters ?? []) {
      addFiltersToParams(params, 'filter', filter);
    }

    final response = await _client.fetch(
      '${_recordApi}/${_name}',
      queryParams: params,
    );

    return ListResponse.fromJson(jsonDecode(response.body));
  }

  Future<Map<String, dynamic>> read(RecordId id, {List<String>? expand}) async {
    final response = await switch (expand) {
      null => _client.fetch('${_recordApi}/${_name}/${id}'),
      _ => _client.fetch('${_recordApi}/${_name}/${id}', queryParams: {
          'expand': expand.join(','),
        })
    };
    return jsonDecode(response.body);
  }

  Future<RecordId> create(Map<String, dynamic> record) async {
    final response = await _client.fetch(
      '${_recordApi}/${_name}',
      method: Method.post,
      body: jsonEncode(record),
    );

    final responseIds = ResponseRecordIds.fromJson(jsonDecode(response.body));
    assert(responseIds._ids.length == 1);
    return responseIds.toRecordIds()[0];
  }

  Future<List<RecordId>> createBulk(List<Map<String, dynamic>> records) async {
    final response = await _client.fetch(
      '${_recordApi}/${_name}',
      method: Method.post,
      body: jsonEncode(records),
    );

    final responseIds = ResponseRecordIds.fromJson(jsonDecode(response.body));
    return responseIds.toRecordIds();
  }

  CreateOperation createOp(Map<String, dynamic> record) {
    return CreateOperation(apiName: _name, value: record);
  }

  Future<void> update(
    RecordId id,
    Map<String, dynamic> record,
  ) async {
    await _client.fetch(
      '${_recordApi}/${_name}/${id}',
      method: Method.patch,
      body: jsonEncode(record),
    );
  }

  UpdateOperation updateOp(
    RecordId id,
    Map<String, dynamic> record,
  ) {
    return UpdateOperation(apiName: _name, id: id, value: record);
  }

  Future<void> delete(RecordId id) async {
    await _client.fetch(
      '${_recordApi}/${_name}/${id}',
      method: Method.delete,
    );
  }

  DeleteOperation deleteOp(RecordId id) {
    return DeleteOperation(apiName: _name, id: id);
  }

  Future<Stream<Event>> subscribe(RecordId id) async {
    return await implSubscribeSse(
      client: _client,
      apiName: _name,
      id: id.toString(),
    );
  }

  Future<Stream<Event>> subscribeAll({
    List<FilterBase>? filters,
  }) async {
    return await implSubscribeSse(
      client: _client,
      apiName: _name,
      id: '*',
      filters: filters,
    );
  }

  Uri imageUri(RecordId id, String column, {String? filename}) {
    if (filename != null) {
      return _client.site().replace(
          path: '${_recordApi}/${_name}/${id}/files/${column}/${filename}');
    }
    return _client
        .site()
        .replace(path: '${_recordApi}/${_name}/${id}/file/${column}');
  }
}

void addFiltersToParams(
  Map<String, String> params,
  String path,
  FilterBase filter,
) {
  final _ = switch (filter) {
    Filter(column: final c, op: final op, value: final v) => () {
        if (op != null) {
          params['${path}[${c}][${_opToString(op)}]'] = switch (op) {
            CompareOp.isNull => 'NULL',
            CompareOp.isNotNull => '!NULL',
            _ => v,
          };
        } else {
          params['${path}[${c}]'] = v;
        }
      }(),
    And(filters: final filters) => () {
        filters.asMap().forEach((index, filter) {
          addFiltersToParams(params, '${path}[\$and][${index}]', filter);
        });
      }(),
    Or(filters: final filters) => () {
        filters.asMap().forEach((index, filter) {
          addFiltersToParams(params, '${path}[\$or][${index}]', filter);
        });
      }(),
  };
}

class ResponseRecordIds {
  final List<String> _ids;

  const ResponseRecordIds(this._ids);

  ResponseRecordIds.fromJson(Map<String, dynamic> json)
      : _ids = (json['ids'] as List).cast<String>();

  List<RecordId> toRecordIds() {
    return _ids.map(RecordId.parse).toList();
  }

  @override
  String toString() => _ids.toString();
}

String _opToString(CompareOp op) {
  return switch (op) {
    CompareOp.equal => '\$eq',
    CompareOp.notEqual => '\$ne',
    CompareOp.lessThan => '\$lt',
    CompareOp.lessThanEqual => '\$lte',
    CompareOp.greaterThan => '\$gt',
    CompareOp.greaterThanEqual => '\$gte',
    CompareOp.like => '\$like',
    CompareOp.regexp => '\$re',
    CompareOp.stWithin => '@within',
    CompareOp.stIntersects => '@intersects',
    CompareOp.stContains => '@contains',
    CompareOp.isNull => '\$is',
    CompareOp.isNotNull => '\$is',
  };
}

const String _recordApi = 'api/records/v1';
