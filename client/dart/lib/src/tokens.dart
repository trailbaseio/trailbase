/// Auth tokens: auth JWT, refresh & CSRF.
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

class MultiFactorAuthToken {
  final String token;

  const MultiFactorAuthToken(this.token);

  MultiFactorAuthToken.fromJson(Map<String, dynamic> json)
      : token = json['mfa_token'];

  @override
  bool operator ==(Object other) {
    return other is MultiFactorAuthToken && token == other.token;
  }

  @override
  int get hashCode => Object.hash(token, null);

  @override
  String toString() => 'MultiFactorAuthToken(${token})';
}
