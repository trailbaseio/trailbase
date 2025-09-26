import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:jwt_decoder/jwt_decoder.dart';
import 'package:logging/logging.dart';
import 'package:http/http.dart' as http;

class User {
  final String id;
  final String email;

  const User({
    required this.id,
    required this.email,
  });

  User.fromJson(Map<String, dynamic> json)
      : id = json['sub'],
        email = json['email'];

  @override
  String toString() => 'User(id=${id}, email=${email})';
}

class Tokens {
  final String auth;
  final String? refresh;
  final String? csrf;

  const Tokens(this.auth, this.refresh, this.csrf);

  Tokens.fromJson(Map<String, dynamic> json)
      : auth = json['auth_token'],
        refresh = json['refresh_token'],
        csrf = json['csrf_token'];

  Map<String, dynamic> toJson() => {
        'auth_token': auth,
        'refresh_token': refresh,
        'csrf_token': csrf,
      };

  bool get valid => JwtDecoder.decode(auth).isNotEmpty;

  @override
  bool operator ==(Object other) {
    return other is Tokens &&
        auth == other.auth &&
        refresh == other.refresh &&
        csrf == other.csrf;
  }

  @override
  int get hashCode => Object.hash(auth, refresh, csrf);

  @override
  String toString() => 'Tokens(${auth}, ${refresh}, ${csrf})';
}

class JwtToken {
  final String sub;
  final int iat;
  final int exp;
  final String email;
  final String csrfToken;

  const JwtToken({
    required this.sub,
    required this.iat,
    required this.exp,
    required this.email,
    required this.csrfToken,
  });

  JwtToken.fromJson(Map<String, dynamic> json)
      : sub = json['sub'],
        iat = json['iat'],
        exp = json['exp'],
        email = json['email'],
        csrfToken = json['csrf_token'];
}

class _TokenState {
  final (Tokens, JwtToken)? state;
  final Map<String, String> headers;

  const _TokenState(this.state, this.headers);

  static _TokenState build(Tokens? tokens) {
    return _TokenState(
      tokens != null
          ? (tokens, JwtToken.fromJson(JwtDecoder.decode(tokens.auth)))
          : null,
      buildHeaders(tokens),
    );
  }
}

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

abstract class RecordId {
  @override
  String toString();

  factory RecordId.integer(int id) => _IntegerRecordId(id);
  factory RecordId.uuid(String id) => _UuidRecordId(id);
}

class _ResponseRecordIds {
  final List<String> _ids;

  const _ResponseRecordIds(this._ids);

  _ResponseRecordIds.fromJson(Map<String, dynamic> json)
      : _ids = (json['ids'] as List).cast<String>();

  List<RecordId> toRecordIds() {
    return _ids.map(toRecordId).toList();
  }

  static RecordId toRecordId(String id) {
    final intId = int.tryParse(id);
    if (intId != null) {
      return _IntegerRecordId(intId);
    }
    return _UuidRecordId(id);
  }

  @override
  String toString() => _ids.toString();
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

abstract class Event {
  Event();

  Map<String, dynamic>? value();

  static Event fromJson(Map<String, dynamic> json) {
    final insert = json['Insert'];
    if (insert != null) {
      return InsertEvent(insert as Map<String, dynamic>);
    }

    final update = json['Update'];
    if (update != null) {
      return UpdateEvent(update as Map<String, dynamic>);
    }

    final delete = json['Delete'];
    if (delete != null) {
      return DeleteEvent(delete as Map<String, dynamic>);
    }

    final error = json['Error'];
    if (error != null) {
      return ErrorEvent(error as String);
    }
    throw Exception('Failed to parse event: ${json}');
  }
}

class InsertEvent extends Event {
  final Map<String, dynamic>? _value;

  InsertEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'InsertEvent(${_value})';
}

class UpdateEvent extends Event {
  final Map<String, dynamic>? _value;

  UpdateEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'UpdateEvent(${_value})';
}

class DeleteEvent extends Event {
  final Map<String, dynamic>? _value;

  DeleteEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'DeleteEvent(${_value})';
}

class ErrorEvent extends Event {
  final String _error;

  ErrorEvent(this._error);

  @override
  Map<String, dynamic>? value() => null;

  @override
  String toString() => 'ErrorEvent(${_error})';
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
}

String _opToSring(CompareOp op) {
  return switch (op) {
    CompareOp.equal => '\$eq',
    CompareOp.notEqual => '\$ne',
    CompareOp.lessThan => '\$lt',
    CompareOp.lessThanEqual => '\$lte',
    CompareOp.greaterThan => '\$gt',
    CompareOp.greaterThanEqual => '\$gte',
    CompareOp.like => '\$like',
    CompareOp.regexp => '\$re',
  };
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
  static const String _recordApi = 'api/records/v1';

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
      _addFiltersToParams(params, 'filter', filter);
    }

    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      queryParams: params,
    );

    return ListResponse.fromJson(jsonDecode(response.body));
  }

  Future<Map<String, dynamic>> read(RecordId id, {List<String>? expand}) async {
    final response = await switch (expand) {
      null => _client.fetch('${RecordApi._recordApi}/${_name}/${id}'),
      _ =>
        _client.fetch('${RecordApi._recordApi}/${_name}/${id}', queryParams: {
          'expand': expand.join(','),
        })
    };
    return jsonDecode(response.body);
  }

  Future<RecordId> create(Map<String, dynamic> record) async {
    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      method: 'POST',
      data: jsonEncode(record),
    );

    if (response.statusCode > 200) {
      throw Exception('[${response.statusCode}] ${response.body}');
    }
    final responseIds = _ResponseRecordIds.fromJson(jsonDecode(response.body));
    assert(responseIds._ids.length == 1);
    return responseIds.toRecordIds()[0];
  }

  Future<List<RecordId>> createBulk(List<Map<String, dynamic>> records) async {
    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      method: 'POST',
      data: jsonEncode(records),
    );

    if (response.statusCode > 200) {
      throw Exception('[${response.statusCode}] ${response.body}');
    }
    final responseIds = _ResponseRecordIds.fromJson(jsonDecode(response.body));
    return responseIds.toRecordIds();
  }

  Future<void> update(
    RecordId id,
    Map<String, dynamic> record,
  ) async {
    await _client.fetch(
      '${RecordApi._recordApi}/${_name}/${id}',
      method: 'PATCH',
      data: jsonEncode(record),
    );
  }

  Future<void> delete(RecordId id) async {
    await _client.fetch(
      '${RecordApi._recordApi}/${_name}/${id}',
      method: 'DELETE',
    );
  }

  Future<Stream<Event>> subscribe(RecordId id) async {
    return await _subscribeImpl(id: id);
  }

  Future<Stream<Event>> subscribeAll({
    List<FilterBase>? filters,
  }) async {
    return await _subscribeImpl(id: '*'.id(), filters: filters);
  }

  Future<Stream<Event>> _subscribeImpl({
    required RecordId id,
    List<FilterBase>? filters,
  }) async {
    final params = <String, String>{};
    for (final filter in filters ?? []) {
      _addFiltersToParams(params, 'filter', filter);
    }

    var tokenState = _client._tokenState;
    final refreshToken = Client._shouldRefresh(tokenState);
    if (refreshToken != null) {
      tokenState =
          _client._tokenState = await _client._refreshTokensImpl(refreshToken);
    }

    final stream = await _client._client.sse(
      '${RecordApi._recordApi}/${_name}/subscribe/${id}',
      tokenState,
      queryParams: params,
    );
    return stream.expand(_decodeEvent);
  }

  Uri imageUri(RecordId id, String colName, {int? index}) {
    if (index != null) {
      return Uri.parse(
          '${_client.site()}/${RecordApi._recordApi}/${_name}/${id}/file/${colName}/${index}');
    }
    return Uri.parse(
        '${_client.site()}/${RecordApi._recordApi}/${_name}/${id}/file/${colName}');
  }
}

class _ThinClient {
  static final _http = http.Client();
  final Uri site;

  const _ThinClient(this.site);

  Future<http.Response> fetch(
    String path,
    _TokenState tokenState, {
    String? data,
    String? method,
    Map<String, String>? queryParams,
  }) async {
    final uri = site.replace(path: path, queryParameters: queryParams);
    final headers = tokenState.headers;

    final response = switch (method ?? 'GET') {
      'GET' => await _http.get(uri, headers: headers),
      'POST' => await _http.post(uri, headers: headers, body: data),
      'PATCH' => await _http.patch(uri, headers: headers, body: data),
      'DELETE' => await _http.delete(uri, headers: headers, body: data),
      _ => throw Exception('unknown method: ${method}'),
    };

    return response;
  }

  Future<Stream<Uint8List>> sse(
    String path,
    _TokenState tokenState, {
    Map<String, String>? queryParams,
  }) async {
    // NOTE: We could use a `StreamController.broadcast()` and track existing
    // streams keyed by `uri` to merge multiple concurrent subscriptions.
    final uri = site.replace(path: path, queryParameters: queryParams);
    final request = http.Request('GET', uri)
      ..headers.addAll(tokenState.headers);

    final response = await _http.send(request);
    if (response.statusCode != 200) {
      throw Exception('[${response.statusCode}] ${response}');
    }

    final buffer = BytesBuilder();
    final sink = StreamController<Uint8List>();

    response.stream.listen(
      (List<int> data) {
        if (_endsWithNewlineNewline(data)) {
          if (buffer.isNotEmpty) {
            buffer.add(data);
            sink.add(buffer.takeBytes());
          } else {
            sink.add(Uint8List.fromList(data));
          }
        } else {
          buffer.add(data);
        }
      },
      onDone: () => sink.close(),
      onError: (error) => sink.addError(error),
      cancelOnError: true,
    );

    return sink.stream;
  }
}

class Client {
  static const String _authApi = 'api/auth/v1';

  final _ThinClient _client;
  final String _site;
  _TokenState _tokenState;
  final void Function(Client, Tokens?)? _authChange;

  Client._(
    String site, {
    Tokens? tokens,
    void Function(Client, Tokens?)? onAuthChange,
  })  : _client = _ThinClient(Uri.parse(site)),
        _site = site,
        _tokenState = _TokenState.build(tokens),
        _authChange = onAuthChange;

  Client(
    String site, {
    void Function(Client, Tokens?)? onAuthChange,
  }) : this._(site, onAuthChange: onAuthChange);

  static Future<Client> withTokens(String site, Tokens tokens,
      {void Function(Client, Tokens?)? onAuthChange}) async {
    final client = Client(site, onAuthChange: onAuthChange);

    try {
      final statusResponse = await client._client
          .fetch('${_authApi}/status', _TokenState.build(tokens));
      final Map<String, dynamic> response = jsonDecode(statusResponse.body);

      final newTokens = Tokens(
        response['auth_token'],
        tokens.refresh,
        response['csrf_token'],
      );
      client._tokenState = _TokenState.build(newTokens);
      client._authChange?.call(client, newTokens);
    } catch (err) {
      // Do nothing
    }

    return client;
  }

  /// Access to the raw tokens, can be used to persist login state.
  Tokens? tokens() => _tokenState.state?.$1;
  User? user() {
    final authToken = tokens()?.auth;
    if (authToken != null) {
      return User.fromJson(JwtDecoder.decode(authToken));
    }
    return null;
  }

  String site() => _site;

  RecordApi records(String name) => RecordApi(this, name);

  _TokenState _updateTokens(Tokens? tokens) {
    final state = _TokenState.build(tokens);

    _tokenState = state;
    _authChange?.call(this, state.state?.$1);

    final claims = state.state?.$2;
    if (claims != null) {
      final now = DateTime.now().millisecondsSinceEpoch / 1000;
      if (claims.exp < now) {
        _logger.warning('Token expired');
      }
    }

    return state;
  }

  Future<Tokens> login(String email, String password) async {
    final response = await fetch(
      '${_authApi}/login',
      method: 'POST',
      data: jsonEncode({
        'email': email,
        'password': password,
      }),
    );

    final Map<String, dynamic> json = jsonDecode(response.body);
    final tokens = Tokens(
      json['auth_token']!,
      json['refresh_token'],
      json['csrf_token'],
    );

    _updateTokens(tokens);
    return tokens;
  }

  Future<Tokens> loginWithAuthCode(
    String authCode, {
    String? pkceCodeVerifier,
  }) async {
    final response = await fetch(
      '${Client._authApi}/token',
      method: 'POST',
      data: jsonEncode({
        'authorization_code': authCode,
        'pkce_code_verifier': pkceCodeVerifier,
      }),
    );

    final Map<String, dynamic> tokenResponse = jsonDecode(response.body);
    final tokens = Tokens(
      tokenResponse['auth_token']!,
      tokenResponse['refresh_token']!,
      tokenResponse['csrf_token'],
    );

    _updateTokens(tokens);
    return tokens;
  }

  Future<bool> logout() async {
    final refreshToken = _tokenState.state?.$1.refresh;
    try {
      if (refreshToken != null) {
        await fetch('${_authApi}/logout',
            method: 'POST',
            data: jsonEncode({
              'refresh_token': refreshToken,
            }));
      } else {
        await fetch('${_authApi}/logout');
      }
    } catch (err) {
      _logger.warning(err);
    }
    _updateTokens(null);
    return true;
  }

  Future<void> deleteUser() async {
    await fetch('${Client._authApi}/delete');
    _updateTokens(null);
  }

  Future<void> changeEmail(String email) async {
    await fetch(
      '${Client._authApi}/change_email',
      method: 'POST',
      data: jsonEncode({
        'new_email': email,
      }),
    );
  }

  Future<void> refreshAuthToken() async {
    final refreshToken = _shouldRefresh(_tokenState);
    if (refreshToken != null) {
      _tokenState = await _refreshTokensImpl(refreshToken);
    }
  }

  Future<_TokenState> _refreshTokensImpl(String refreshToken) async {
    final response = await _client.fetch(
      '${_authApi}/refresh',
      _tokenState,
      method: 'POST',
      data: jsonEncode({
        'refresh_token': refreshToken,
      }),
    );

    final Map<String, dynamic> tokenResponse = jsonDecode(response.body);
    return _TokenState.build(Tokens(
      tokenResponse['auth_token']!,
      refreshToken,
      tokenResponse['csrf_token'],
    ));
  }

  static String? _shouldRefresh(_TokenState tokenState) {
    final state = tokenState.state;
    final now = DateTime.now().millisecondsSinceEpoch / 1000;
    if (state != null && state.$2.exp - 60 < now) {
      return state.$1.refresh;
    }
    return null;
  }

  Future<http.Response> fetch(
    String path, {
    bool? throwOnError,
    String? data,
    String? method,
    Map<String, String>? queryParams,
  }) async {
    var tokenState = _tokenState;
    final refreshToken = _shouldRefresh(tokenState);
    if (refreshToken != null) {
      tokenState = _tokenState = await _refreshTokensImpl(refreshToken);
    }

    final response = await _client.fetch(
      path,
      tokenState,
      data: data,
      method: method,
      queryParams: queryParams,
    );

    if (response.statusCode != 200 && (throwOnError ?? true)) {
      final errMsg = response.body;
      throw Exception('[${response.statusCode}]: ${errMsg}');
    }

    return response;
  }
}

Map<String, String> buildHeaders(Tokens? tokens) {
  final Map<String, String> base = {
    'Content-Type': 'application/json',
  };

  if (tokens != null) {
    base['Authorization'] = 'Bearer ${tokens.auth}';

    final refresh = tokens.refresh;
    if (refresh != null) {
      base['Refresh-Token'] = refresh;
    }

    final csrf = tokens.csrf;
    if (csrf != null) {
      base['CSRF-Token'] = csrf;
    }
  }

  return base;
}

void _addFiltersToParams(
    Map<String, dynamic> params, String path, FilterBase filter) {
  final _ = switch (filter) {
    Filter(column: final c, op: final op, value: final v) => () {
        if (op != null) {
          params['${path}[${c}][${_opToSring(op)}]'] = v;
        } else {
          params['${path}[${c}]'] = v;
        }
      }(),
    And(filters: final filters) => () {
        filters.asMap().forEach((index, filter) {
          _addFiltersToParams(params, '${path}[\$and][${index}]', filter);
        });
      }(),
    Or(filters: final filters) => () {
        filters.asMap().forEach((index, filter) {
          _addFiltersToParams(params, '${path}[\$or][${index}]', filter);
        });
      }(),
  };
}

bool _endsWithNewlineNewline(List<int> bytes) {
  if (bytes.length >= 2) {
    return bytes[bytes.length - 1] == 10 && bytes[bytes.length - 2] == 10;
  }
  return false;
}

List<Event> _decodeEvent(Uint8List bytes) {
  final decoded = utf8.decode(bytes);
  if (decoded.startsWith('data: ')) {
    // Cut off "data: " and decode.
    return [Event.fromJson(jsonDecode(decoded.substring(6)))];
  }

  // Heart-beat, do nothing.
  return [];
}

final _logger = Logger('trailbase');
