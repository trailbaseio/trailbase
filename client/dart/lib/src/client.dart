import 'dart:async';
import 'dart:convert';

import 'package:meta/meta.dart';
import 'package:logging/logging.dart';
import 'package:http/http.dart' as http;

import './record_api.dart';
import './sse.dart';
import './token_state.dart';
import './tokens.dart';
import './transport.dart';
import './user.dart';

class Client {
  final Uri _baseUrl;
  final Transport _transport;

  TokenState _tokenState;
  final void Function(Client, Tokens?)? _authChange;

  @visibleForTesting
  final Map<String, dynamic> cache = {};

  Client._(
    String site, {
    Tokens? tokens,
    Transport? transport,
    void Function(Client, Tokens?)? onAuthChange,
  })  : _baseUrl = Uri.parse(site),
        _transport = transport ?? DefaultTransport(url: Uri.parse(site)),
        _tokenState = TokenState.build(tokens),
        _authChange = onAuthChange;

  Client(
    String site, {
    void Function(Client, Tokens?)? onAuthChange,
    Transport? transport,
  }) : this._(site, transport: transport, onAuthChange: onAuthChange);

  static Future<Client> withTokens(
    String site,
    Tokens tokens, {
    Transport? transport,
    void Function(Client, Tokens?)? onAuthChange,
  }) async {
    final client =
        Client(site, transport: transport, onAuthChange: onAuthChange);

    // Initial check if tokens are valid and potentially refresh auth token.
    // Do not use _updateToken to not call [onAuthChange] on initial tokens.
    client._tokenState = TokenState.build(tokens);
    await client.refreshAuthToken();

    return client;
  }

  /// TrailBase server's address.
  Uri site() => _baseUrl;

  /// Access to the raw tokens, can be used to persist login state.
  Tokens? tokens() => _tokenState.state?.$1;

  /// Currently logged-in user.
  User? user() => _tokenState.user();

  /// Accessor for Record APIs with given [name].
  RecordApi records(String name) => RecordApi(this, name);

  Future<MultiFactorAuthToken?> login(
    String emailOrUsername,
    String password,
  ) async {
    final response = await fetch(
      '${_authApi}/login',
      method: Method.post,
      body: jsonEncode({
        'email_or_username': emailOrUsername,
        'password': password,
      }),
      throwOnError: false,
    );

    if (response.statusCode == 403) {
      return MultiFactorAuthToken.fromJson(jsonDecode(response.body));
    } else if (response.statusCode != 200) {
      throw HttpException(response.statusCode, response.body);
    }

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
    return null;
  }

  Future<void> loginWithAuthCode(
    String authCode, {
    String? pkceCodeVerifier,
  }) async {
    final response = await fetch(
      '${_authApi}/token',
      method: Method.post,
      body: jsonEncode({
        'authorization_code': authCode,
        'pkce_code_verifier': pkceCodeVerifier,
      }),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
  }

  Future<void> loginSecond(MultiFactorAuthToken token, String code) async {
    final response = await fetch(
      '${_authApi}/login_mfa',
      method: Method.post,
      body: jsonEncode({
        'mfa_token': token.token,
        'totp': code,
      }),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
  }

  Future<void> requestOtp(String emailOrUsername) async {
    await fetch(
      '${_authApi}/otp/request',
      method: Method.post,
      body: jsonEncode({
        'email_or_username': emailOrUsername,
        'redirect_uri': null,
      }),
    );
  }

  Future<void> loginOtp(String email, String code) async {
    final response = await fetch(
      '${_authApi}/otp/login',
      method: Method.post,
      body: jsonEncode({
        'email': email,
        'code': code,
      }),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
  }

  Future<void> loginAnonymously() async {
    final response = await fetch(
      '${_authApi}/login_anonymous',
      method: Method.post,
      body: jsonEncode({}),
    );

    final tokens = Tokens.fromJson(jsonDecode(response.body));
    _updateTokens(tokens);
  }

  Future<void> logout() async {
    try {
      final refreshToken = _tokenState.state?.$1.refresh;
      if (refreshToken != null) {
        await fetch('${_authApi}/logout',
            method: Method.post,
            body: jsonEncode({
              'refresh_token': refreshToken,
            }));
      } else {
        await fetch('${_authApi}/logout');
      }
    } finally {
      _updateTokens(null);
    }
  }

  Future<void> promoteAnonymous({
    required String password,
    String? email,
    String? username,
  }) async {
    await fetch(
      '${_authApi}/promote_anonymous',
      method: Method.post,
      body: jsonEncode({
        'new_password': password,
        'new_email': email,
        'new_username': username,
      }),
    );
  }

  Future<void> refreshAuthToken() async {
    final refreshToken = _tokenState.shouldRefresh();
    if (refreshToken != null) {
      _tokenState = await _refreshTokensImpl(
        _transport,
        _tokenState.headers,
        refreshToken,
      );
    }
  }

  Future<http.Response> fetch(
    String path, {
    Method method = Method.get,
    String? body,
    Map<String, dynamic>? queryParams,
    bool throwOnError = true,
  }) async {
    final refreshToken = _tokenState.shouldRefresh();
    if (refreshToken != null) {
      _tokenState = await _refreshTokensImpl(
        _transport,
        _tokenState.headers,
        refreshToken,
      );
    }

    final response = await _transport.fetch(
      path,
      method: method,
      headers: _tokenState.headers,
      body: body,
      queryParams: queryParams,
    );

    if (response.statusCode != 200 && throwOnError) {
      throw HttpException(response.statusCode, response.body);
    }

    return response;
  }

  TokenState _updateTokens(Tokens? tokens) {
    final oldTokens = _tokenState.state?.$1;
    if (oldTokens == tokens) {
      return _tokenState;
    }

    final state = _tokenState = TokenState.build(tokens);

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
}

Future<Stream<Event>> implSubscribeSse({
  required Client client,
  required String apiName,
  required String id,
  List<FilterBase>? filters,
}) async {
  final params = <String, String>{};
  for (final filter in filters ?? []) {
    addFiltersToParams(params, 'filter', filter);
  }

  final refreshToken = client._tokenState.shouldRefresh();
  if (refreshToken != null) {
    client._tokenState = await _refreshTokensImpl(
      client._transport,
      client._tokenState.headers,
      refreshToken,
    );
  }

  final uri = client._baseUrl.replace(
      path: '${_recordApi}/${apiName}/subscribe/${id}',
      queryParameters: params);

  return await connectSse(
    client._transport,
    uri,
    headers: client._tokenState.headers,
    cache: client.cache.cast<String, Future<StreamController<Event>>>(),
  );
}

Future<TokenState> _refreshTokensImpl(
  Transport transport,
  Map<String, String> headers,
  String refreshToken,
) async {
  // NOTE: We cannot use `Client.fetch`, which may refresh tokens to prevent a loop.
  final response = await transport.fetch('${_authApi}/refresh',
      method: Method.post,
      headers: headers,
      body: jsonEncode({
        'refresh_token': refreshToken,
      }));

  return switch (response.statusCode) {
    200 => TokenState.build(Tokens.fromJson(jsonDecode(response.body))),
    // If the refresh token got rejected, there's no way to recover. Might as well log out.
    401 => TokenState.build(null),
    _ => throw HttpException(response.statusCode, response.body),
  };
}

final _logger = Logger('trailbase');
const String _authApi = 'api/auth/v1';
const String _recordApi = 'api/records/v1';
