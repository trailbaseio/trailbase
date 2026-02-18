import 'dart:async';
import 'dart:convert';

import 'package:meta/meta.dart';
import 'package:jwt_decoder/jwt_decoder.dart';
import 'package:logging/logging.dart';
import 'package:http/http.dart' as http;

import './sse.dart';

class User {
  final String id;
  final String email;

  const User({
    required this.id,
    required this.email,
  });

  @override
  String toString() => 'User(id=${id}, email=${email})';

  @override
  bool operator ==(Object other) {
    return other is User && id == other.id && email == other.email;
  }

  @override
  int get hashCode => Object.hash(id, email);
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
      data: jsonEncode(record),
    );

    final responseIds = _ResponseRecordIds.fromJson(jsonDecode(response.body));
    assert(responseIds._ids.length == 1);
    return responseIds.toRecordIds()[0];
  }

  Future<List<RecordId>> createBulk(List<Map<String, dynamic>> records) async {
    final response = await _client.fetch(
      '${_recordApi}/${_name}',
      method: Method.post,
      data: jsonEncode(records),
    );

    final responseIds = _ResponseRecordIds.fromJson(jsonDecode(response.body));
    return responseIds.toRecordIds();
  }

  Future<void> update(
    RecordId id,
    Map<String, dynamic> record,
  ) async {
    await _client.fetch(
      '${_recordApi}/${_name}/${id}',
      method: Method.patch,
      data: jsonEncode(record),
    );
  }

  Future<void> delete(RecordId id) async {
    await _client.fetch(
      '${_recordApi}/${_name}/${id}',
      method: Method.delete,
    );
  }

  Future<Stream<Event>> subscribe(RecordId id) async {
    return await _subscribeImpl(id: id.toString());
  }

  Future<Stream<Event>> subscribeAll({
    List<FilterBase>? filters,
  }) async {
    return await _subscribeImpl(id: '*', filters: filters);
  }

  Future<Stream<Event>> _subscribeImpl({
    required String id,
    List<FilterBase>? filters,
  }) async {
    final params = <String, String>{};
    for (final filter in filters ?? []) {
      addFiltersToParams(params, 'filter', filter);
    }

    final refreshToken = _client._tokenState._shouldRefresh();
    if (refreshToken != null) {
      _client._tokenState = await _client._refreshTokensImpl(refreshToken);
    }

    final uri = _client._site.replace(
        path: '${_recordApi}/${_name}/subscribe/${id}',
        queryParameters: params);
    final request = http.Request('GET', uri)
      ..headers.addAll(_client._tokenState.headers);

    return await connectSse(
      _client._http,
      request,
      cache: _client.cache.cast<String, Future<StreamController<Event>>>(),
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

enum Method {
  get,
  post,
  patch,
  delete,
}

class Client {
  final _http = http.Client();
  final Uri _site;

  _TokenState _tokenState;
  final void Function(Client, Tokens?)? _authChange;

  @visibleForTesting
  final Map<String, dynamic> cache = {};

  Client._(
    String site, {
    Tokens? tokens,
    void Function(Client, Tokens?)? onAuthChange,
  })  : _site = Uri.parse(site),
        _tokenState = _TokenState.build(tokens),
        _authChange = onAuthChange;

  Client(
    String site, {
    void Function(Client, Tokens?)? onAuthChange,
  }) : this._(site, onAuthChange: onAuthChange);

  static Future<Client> withTokens(String site, Tokens tokens,
      {void Function(Client, Tokens?)? onAuthChange}) async {
    final client = Client(site, onAuthChange: onAuthChange);

    // Initial check if tokens are valid and potentially refresh auth token.
    // Do not use _updateToken to not call [onAuthChange] on intial tokens.
    client._tokenState = _TokenState.build(tokens);
    await client.refreshAuthToken();
    // final uri = client.site().replace(path: '${_authApi}/status');
    // final statusResponse =
    //     await client._http.get(uri, headers: _buildHeaders(tokens));
    // client._updateTokens(Tokens.fromJson(jsonDecode(statusResponse.body)));

    return client;
  }

  /// TrailBase server's address.
  Uri site() => _site;

  /// Access to the raw tokens, can be used to persist login state.
  Tokens? tokens() => _tokenState.state?.$1;

  /// Currently logged-in user.
  User? user() => _tokenState.user();

  /// Accessor for Record APIs with given [name].
  RecordApi records(String name) => RecordApi(this, name);

  Future<Tokens> login(String email, String password) async {
    final response = await fetch(
      '${_authApi}/login',
      method: Method.post,
      data: jsonEncode({
        'email': email,
        'password': password,
      }),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
    return tokens;
  }

  Future<Tokens> loginWithAuthCode(
    String authCode, {
    String? pkceCodeVerifier,
  }) async {
    final response = await fetch(
      '${_authApi}/token',
      method: Method.post,
      data: jsonEncode({
        'authorization_code': authCode,
        'pkce_code_verifier': pkceCodeVerifier,
      }),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
    return tokens;
  }

  Future<void> logout() async {
    try {
      final refreshToken = _tokenState.state?.$1.refresh;
      if (refreshToken != null) {
        await fetch('${_authApi}/logout',
            method: Method.post,
            data: jsonEncode({
              'refresh_token': refreshToken,
            }));
      } else {
        await fetch('${_authApi}/logout');
      }
    } finally {
      _updateTokens(null);
    }
  }

  // Future<void> deleteUser() async {
  //   await fetch('${_authApi}/delete');
  //   _updateTokens(null);
  // }
  //
  // Future<void> changeEmail(String email) async {
  //   await fetch(
  //     '${_authApi}/change_email',
  //     method: Method.post,
  //     data: jsonEncode({
  //       'new_email': email,
  //     }),
  //   );
  // }

  Future<void> refreshAuthToken() async {
    final refreshToken = _tokenState._shouldRefresh();
    if (refreshToken != null) {
      _tokenState = await _refreshTokensImpl(refreshToken);
    }
  }

  Future<http.Response> fetch(
    String path, {
    Method method = Method.get,
    String? data,
    Map<String, dynamic>? queryParams,
    bool throwOnError = true,
  }) async {
    final refreshToken = _tokenState._shouldRefresh();
    if (refreshToken != null) {
      _tokenState = await _refreshTokensImpl(refreshToken);
    }

    final uri = _site.replace(path: path, queryParameters: queryParams);
    final headers = _tokenState.headers;

    final response = switch (method) {
      Method.get => await _http.get(uri, headers: headers),
      Method.post => await _http.post(uri, headers: headers, body: data),
      Method.patch => await _http.patch(uri, headers: headers, body: data),
      Method.delete => await _http.delete(uri, headers: headers, body: data),
    };

    if (response.statusCode != 200 && throwOnError) {
      throw HttpException(response.statusCode, response.body);
    }

    return response;
  }

  _TokenState _updateTokens(Tokens? tokens) {
    final oldTokens = _tokenState.state?.$1;
    if (oldTokens == tokens) {
      return _tokenState;
    }

    final state = _tokenState = _TokenState.build(tokens);

    _authChange?.call(this, tokens);

    final claims = state.state?.$2;
    if (claims != null) {
      final now = DateTime.now().millisecondsSinceEpoch / 1000;
      if (claims.exp < now) {
        _logger.warning('Token expired');
      }
    }

    return state;
  }

  Future<_TokenState> _refreshTokensImpl(String refreshToken) async {
    final uri = site().replace(path: '${_authApi}/refresh');
    final response = await _http.post(uri,
        headers: _tokenState.headers,
        body: jsonEncode({
          'refresh_token': refreshToken,
        }));

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    assert(tokens.refresh == refreshToken);
    return _updateTokens(tokens);
  }
}

Map<String, String> _buildHeaders(Tokens? tokens) {
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

class _JwtToken {
  final String sub;
  final int iat;
  final int exp;
  final String email;
  final String csrfToken;

  const _JwtToken({
    required this.sub,
    required this.iat,
    required this.exp,
    required this.email,
    required this.csrfToken,
  });

  factory _JwtToken.fromAuthToken(String token) =>
      _JwtToken.fromJson(JwtDecoder.decode(token));

  _JwtToken.fromJson(Map<String, dynamic> json)
      : sub = json['sub'],
        iat = json['iat'],
        exp = json['exp'],
        email = json['email'],
        csrfToken = json['csrf_token'];
}

class _TokenState {
  final (Tokens, _JwtToken)? state;
  final Map<String, String> headers;

  const _TokenState(this.state, this.headers);

  static _TokenState build(Tokens? tokens) {
    return _TokenState(
      tokens != null ? (tokens, _JwtToken.fromAuthToken(tokens.auth)) : null,
      _buildHeaders(tokens),
    );
  }

  User? user() {
    final jwt = state?.$2;
    return (jwt != null) ? User(id: jwt.sub, email: jwt.email) : null;
  }

  /// Returns refresh token if refresh is warranted.
  String? _shouldRefresh() {
    final s = state;
    if (s != null) {
      final now = DateTime.now().millisecondsSinceEpoch / 1000;
      if (s.$2.exp - 60 < now) {
        return s.$1.refresh;
      }
    }
    return null;
  }
}

@visibleForTesting
void addFiltersToParams(
    Map<String, String> params, String path, FilterBase filter) {
  final _ = switch (filter) {
    Filter(column: final c, op: final op, value: final v) => () {
        if (op != null) {
          params['${path}[${c}][${_opToString(op)}]'] = v;
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

final _logger = Logger('trailbase');
const String _authApi = 'api/auth/v1';
const String _recordApi = 'api/records/v1';
