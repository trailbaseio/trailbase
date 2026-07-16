import 'package:jwt_decoder/jwt_decoder.dart';

import './user.dart';
import './tokens.dart';

class TokenState {
  final (Tokens, _JwtToken)? state;
  final Map<String, String> headers;

  const TokenState(this.state, this.headers);

  static TokenState build(Tokens? tokens) {
    return TokenState(
      tokens != null ? (tokens, _JwtToken.fromAuthToken(tokens.auth)) : null,
      _buildHeaders(tokens),
    );
  }

  User? user() {
    final jwt = state?.$2;
    return (jwt != null)
        ? User(id: jwt.sub, email: jwt.email, username: jwt.username)
        : null;
  }

  /// Returns refresh token if refresh is warranted.
  String? shouldRefresh() {
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

class _JwtToken {
  final String sub;
  final int iat;
  final int exp;
  final String? email;
  final String? username;
  final String csrfToken;

  const _JwtToken({
    required this.sub,
    required this.iat,
    required this.exp,
    required this.email,
    required this.username,
    required this.csrfToken,
  });

  factory _JwtToken.fromAuthToken(String token) =>
      _JwtToken.fromJson(JwtDecoder.decode(token));

  _JwtToken.fromJson(Map<String, dynamic> json)
      : sub = json['sub'],
        iat = json['iat'],
        exp = json['exp'],
        email = json['email'],
        username = json['username'],
        csrfToken = json['csrf_token'];
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
