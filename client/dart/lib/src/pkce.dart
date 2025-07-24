import 'dart:convert';
import 'dart:math';

import 'package:crypto/crypto.dart';

/// A pair of (pkceCodeVerifier, pkceCodeChallenge).
typedef PkcePair = ({
  /// The random code verifier.
  String verifier,

  /// The code challenge, computed as base64UrlNoPad(sha256(verifier)).
  String challenge
});

extension Pkce on PkcePair {
  /// Generates a [PkcePair].
  ///
  /// [length] is the length used to generate the [verifier]. It must be
  /// between 32 and 96, inclusive, which corresponds to a [verifier] of
  /// length between 43 and 128, inclusive. The spec recommends a length of 32.
  static PkcePair generate({int length = 32}) {
    if (length < 32 || length > 96) {
      throw ArgumentError.value(
        length,
        'length',
        'The length must be between 32 and 96, inclusive.',
      );
    }

    final random = Random.secure();
    final verifier =
        base64UrlEncode(List.generate(length, (_) => random.nextInt(256)))
            .split('=')
            .first;
    final challenge =
        base64UrlEncode(sha256.convert(ascii.encode(verifier)).bytes)
            .split('=')
            .first;

    return (verifier: verifier, challenge: challenge);
  }
}
