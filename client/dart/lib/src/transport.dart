import 'package:http/http.dart' as http;

enum Method {
  get,
  post,
  patch,
  delete,
}

abstract class Transport {
  Future<http.Response> fetch(
    String path, {
    Method method = Method.get,
    Map<String, String>? headers,
    String? body,
    Map<String, dynamic>? queryParams,
  });

  Future<http.StreamedResponse> stream(
    Uri uri, {
    Map<String, String>? headers,
  });
}

class DefaultTransport implements Transport {
  final _http = http.Client();
  final Uri _baseUrl;

  DefaultTransport(this._baseUrl);

  @override
  Future<http.Response> fetch(
    String path, {
    Method method = Method.get,
    Map<String, String>? headers,
    String? body,
    Map<String, dynamic>? queryParams,
  }) async {
    final uri = _baseUrl.replace(path: path, queryParameters: queryParams);
    return switch (method) {
      Method.get => await _http.get(uri, headers: headers),
      Method.post => await _http.post(uri, headers: headers, body: body),
      Method.patch => await _http.patch(uri, headers: headers, body: body),
      Method.delete => await _http.delete(uri, headers: headers, body: body),
    };
  }

  @override
  Future<http.StreamedResponse> stream(
    Uri uri, {
    Map<String, String>? headers,
  }) async {
    final request = http.Request('GET', uri)..headers.addAll(headers ?? {});
    return await _http.send(request);
  }
}
