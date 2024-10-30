import 'dart:io' show Platform;

import 'package:flutter/material.dart';
import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter_web_auth_2/flutter_web_auth_2.dart';
import 'package:logging/logging.dart';
import 'package:trailbase/trailbase.dart';

class LoginFormWidget extends StatefulWidget {
  final Client client;

  const LoginFormWidget({
    super.key,
    required this.client,
  });

  @override
  State<LoginFormWidget> createState() => _LoginFormState();
}

class _LoginFormState extends State<LoginFormWidget> {
  final _usernameCtrl = TextEditingController();
  final _passwordCtrl = TextEditingController();

  @override
  void dispose() {
    _usernameCtrl.dispose();
    _passwordCtrl.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return Container(
      padding: const EdgeInsets.all(8),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 16),
            child: TextFormField(
              controller: _usernameCtrl,
              decoration: const InputDecoration(
                border: UnderlineInputBorder(),
                labelText: 'E-mail',
              ),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: 8, vertical: 16),
            child: TextFormField(
              controller: _passwordCtrl,
              obscureText: true,
              decoration: const InputDecoration(
                border: UnderlineInputBorder(),
                labelText: 'password',
              ),
            ),
          ),
          Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              FilledButton(
                child: const Text('OAuth'),
                onPressed: () async {
                  final scaffold = Scaffold.of(context);
                  final messenger = ScaffoldMessenger.of(context);

                  final redirectUri = _redirectUri();
                  final callbackUrlScheme = _callbackUrlScheme();
                  final (:verifier, :challenge) = Pkce.generate();

                  _logger.info(
                      'redirect: ${redirectUri}; callbackUrlScheme: ${callbackUrlScheme}');

                  // Construct the login page url
                  final url = Uri.parse('${widget.client.site()}/_/auth/login')
                      .replace(queryParameters: {
                    'redirect_to': redirectUri,
                    'response_type': 'code',
                    'pkce_code_challenge': challenge,
                  });

                  // Open a browser or webview to get an authorization code.
                  final result = await FlutterWebAuth2.authenticate(
                    url: url.toString(),
                    callbackUrlScheme: callbackUrlScheme,
                    options: const FlutterWebAuth2Options(
                      useWebview: false,
                    ),
                  );

                  _logger.info('RESULT: ${result}');

                  final String? code =
                      Uri.parse(result).queryParameters['code'];
                  if (code == null) {
                    _logger.warning('Failed to get auth code: ${result}');
                    return;
                  }

                  try {
                    await widget.client.loginWithAuthCode(
                      code,
                      pkceCodeVerifier: verifier,
                    );
                    scaffold.closeEndDrawer();
                  } catch (err) {
                    messenger.showSnackBar(
                      SnackBar(
                        duration: const Duration(seconds: 5),
                        content: Text(err.toString()),
                      ),
                    );
                  }
                },
              ),

              // Password login
              FilledButton(
                child: const Text('Login'),
                onPressed: () {
                  final scaffold = Scaffold.of(context);
                  final messenger = ScaffoldMessenger.of(context);
                  (() async {
                    final client = widget.client;
                    try {
                      await client.login(
                          _usernameCtrl.text, _passwordCtrl.text);
                      scaffold.closeEndDrawer();
                    } catch (err) {
                      messenger.showSnackBar(
                        SnackBar(
                          duration: const Duration(seconds: 5),
                          content: Text(err.toString()),
                        ),
                      );
                    }
                  })();
                },
              ),
            ],
          ),
        ],
      ),
    );
  }
}

String _callbackUrlScheme() {
  if (Platform.isLinux || Platform.isWindows) {
    return 'http://localhost:22342';
  }
  return 'trailbase-example-blog';
}

// See https://pub.dev/packages/flutter_web_auth_2#setup.
String _redirectUri() {
  // On web we redirect to a different page web/auth.html which
  // will then communicate back to `flutter_web_auth_2` via `postMessage()` apis.
  if (kIsWeb) {
    return '${Uri.base}auth.html';
  }

  // `flutter_web_auth_2` will start a local http server to receive the callback on Linux and Windows.
  // Ideally, we'd pick a port that is guaranteed to be available but the entire
  // approach is racy anyway :shrug:.
  if (Platform.isLinux || Platform.isWindows) {
    return 'http://localhost:22342';
  }

  return '${_callbackUrlScheme()}://login-callback';
}

final _logger = Logger('login');
