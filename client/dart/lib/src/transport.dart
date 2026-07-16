import 'package:http/http.dart' as http;

class HttpException implements Exception {
  final int status;
  final String? message;

  const HttpException(this.status, this.message);

  @override
  String toString() => 'HttpException(${status}, "${message}")';
}

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
  final http.Client _http;
  final Uri _baseUrl;

  /// The extra headers provide the means for users to override or inject their
  /// own additional headers (the default doesn't use them). This may for
  /// example be used to control the behavior of a reverse proxy.
  final Map<String, String>? _extraHeaders;

  DefaultTransport({
    required Uri url,
    Map<String, String>? headers,
    http.Client? client,
  })  : _http = client ?? http.Client(),
        _baseUrl = url,
        _extraHeaders = headers;

  Map<String, String>? mergeHeaders(Map<String, String>? headers) {
    if (headers != null) {
      return _extraHeaders != null
          ? {
              ...headers,
              ..._extraHeaders,
            }
          : headers;
    }
    return _extraHeaders;
  }

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
      Method.get => await _http.get(uri, headers: mergeHeaders(headers)),
      Method.post =>
        await _http.post(uri, headers: mergeHeaders(headers), body: body),
      Method.patch =>
        await _http.patch(uri, headers: mergeHeaders(headers), body: body),
      Method.delete =>
        await _http.delete(uri, headers: mergeHeaders(headers), body: body),
    };
  }

  @override
  Future<http.StreamedResponse> stream(
    Uri uri, {
    Map<String, String>? headers,
  }) async {
    final request = http.Request('GET', uri);
    final mergedHeaders = mergeHeaders(headers);
    if (mergedHeaders != null) {
      request.headers.addAll(mergedHeaders);
    }
    return await _http.send(request);
  }
}
