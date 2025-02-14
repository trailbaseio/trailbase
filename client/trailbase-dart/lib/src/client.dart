import 'dart:convert';
import 'dart:typed_data';

import 'package:jwt_decoder/jwt_decoder.dart';
import 'package:logging/logging.dart';
import 'package:dio/dio.dart' as dio;

import 'sse.dart';

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
  final Map<String, dynamic> headers;

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

  const Pagination({
    this.cursor,
    this.limit,
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

class RecordApi {
  static const String _recordApi = 'api/records/v1';

  final String _name;
  final Client _client;

  const RecordApi(this._client, this._name);

  Future<ListResponse> list({
    Pagination? pagination,
    List<String>? order,
    List<String>? filters,
    bool? count,
    List<String>? expand,
  }) async {
    final params = <String, dynamic>{};
    if (pagination != null) {
      final cursor = pagination.cursor;
      if (cursor != null) params['cursor'] = cursor;

      final limit = pagination.limit;
      if (limit != null) params['limit'] = limit.toString();
    }

    if (order != null) params['order'] = order.join(',');
    if (count ?? false) params['count'] = 'true';
    if (expand != null) params['expand'] = expand.join(',');

    if (filters != null) {
      for (final filter in filters) {
        final (nameOp, value) = splitOnce(filter, '=');
        if (value == null) {
          throw Exception(
              'Filter "${filter}" does not match: "name[op]=value"');
        }
        params[nameOp] = value;
      }
    }

    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      queryParams: params,
    );

    return ListResponse.fromJson(response.data);
  }

  Future<Map<String, dynamic>> read(RecordId id, {List<String>? expand}) async {
    final response = await _client.fetch(expand == null
        ? '${RecordApi._recordApi}/${_name}/${id}'
        : '${RecordApi._recordApi}/${_name}/${id}?expand=${expand.join(",")}');
    return response.data;
  }

  Future<RecordId> create(Map<String, dynamic> record) async {
    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      method: 'POST',
      data: record,
    );

    if ((response.statusCode ?? 400) > 200) {
      throw Exception('${response.data} ${response.statusMessage}');
    }
    final responseIds = _ResponseRecordIds.fromJson(response.data);
    assert(responseIds._ids.length == 1);
    return responseIds.toRecordIds()[0];
  }

  Future<List<RecordId>> createBulk(List<Map<String, dynamic>> records) async {
    final response = await _client.fetch(
      '${RecordApi._recordApi}/${_name}',
      method: 'POST',
      data: records,
    );

    if ((response.statusCode ?? 400) > 200) {
      throw Exception('${response.data} ${response.statusMessage}');
    }
    final responseIds = _ResponseRecordIds.fromJson(response.data);
    return responseIds.toRecordIds();
  }

  Future<void> update(
    RecordId id,
    Map<String, dynamic> record,
  ) async {
    await _client.fetch(
      '${RecordApi._recordApi}/${_name}/${id}',
      method: 'PATCH',
      data: record,
    );
  }

  Future<void> delete(RecordId id) async {
    await _client.fetch(
      '${RecordApi._recordApi}/${_name}/${id}',
      method: 'DELETE',
    );
  }

  static List<Event> _decodeEvent(Uint8List bytes) {
    final decoded = utf8.decode(bytes);
    if (decoded.startsWith('data: ')) {
      return [Event.fromJson(jsonDecode(decoded.substring(6)))];
    }

    // Heart-beat, do nothing.
    return [];
  }

  Future<Stream<Event>> subscribe(RecordId id) async {
    final resp = await _client.fetch(
      '${RecordApi._recordApi}/${_name}/subscribe/${id}',
      responseType: dio.ResponseType.stream,
    );

    final Stream<Uint8List> stream = resp.data.stream;
    return stream.expand(_decodeEvent);
  }

  Future<Stream<Event>> subscribeAll() async {
    final resp = await _client.fetch(
      '${RecordApi._recordApi}/${_name}/subscribe/*',
      responseType: dio.ResponseType.stream,
    );

    final Stream<Uint8List> stream = resp.data.stream;
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
  static final _dio = dio.Dio()..interceptors.add(SeeInterceptor());

  final String site;

  const _ThinClient(this.site);

  Future<dio.Response> fetch(
    String path,
    _TokenState tokenState, {
    Object? data,
    String? method,
    Map<String, dynamic>? queryParams,
    dio.ResponseType? responseType,
  }) async {
    if (path.startsWith('/')) {
      throw Exception('Path starts with "/". Relative path expected.');
    }

    final response = await _dio.request(
      '${site}/${path}',
      data: data,
      queryParameters: queryParams,
      options: dio.Options(
        method: method,
        headers: tokenState.headers,
        validateStatus: (int? status) => true,
        responseType: responseType,
      ),
    );

    return response;
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
  })  : _client = _ThinClient(site),
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
      final Map<String, dynamic> response = statusResponse.data;

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
      data: {
        'email': email,
        'password': password,
      },
    );

    final Map<String, dynamic> json = response.data;
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
      data: {
        'authorization_code': authCode,
        'pkce_code_verifier': pkceCodeVerifier,
      },
    );

    final Map<String, dynamic> tokenResponse = await response.data;
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
        await fetch('${_authApi}/logout', method: 'POST', data: {
          'refresh_token': refreshToken,
        });
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
      data: {
        'new_email': email,
      },
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
      data: {
        'refresh_token': refreshToken,
      },
    );

    final Map<String, dynamic> tokenResponse = await response.data;
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

  Future<dio.Response> fetch(
    String path, {
    bool? throwOnError,
    Object? data,
    String? method,
    Map<String, dynamic>? queryParams,
    dio.ResponseType? responseType,
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
      responseType: responseType,
    );

    if (response.statusCode != 200 && (throwOnError ?? true)) {
      final errMsg = await response.data;
      throw Exception(
          '[${response.statusCode}] ${response.statusMessage}}: ${errMsg}');
    }

    return response;
  }
}

Map<String, dynamic> buildHeaders(Tokens? tokens) {
  final Map<String, dynamic> base = {
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

(String, String?) splitOnce(String s, Pattern pattern) {
  final int idx = s.indexOf(pattern);
  if (idx < 0) {
    return (s, null);
  }
  return (s.substring(0, idx), s.substring(idx + 1));
}

final _logger = Logger('trailbase');
