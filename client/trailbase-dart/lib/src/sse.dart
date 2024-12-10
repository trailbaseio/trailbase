import 'dart:async';
import 'dart:typed_data';

import 'package:dio/dio.dart' as dio;

class SeeInterceptor extends dio.Interceptor {
  @override
  void onResponse(
      dio.Response response, dio.ResponseInterceptorHandler handler) {
    if (response.requestOptions.responseType == dio.ResponseType.stream) {
      final Stream<Uint8List> stream = response.data.stream;

      final buffer = BytesBuilder();
      final transformedStream = stream.transform<Uint8List>(
        StreamTransformer.fromHandlers(
          handleData: (Uint8List data, EventSink<Uint8List> sink) {
            // If terminated correctly (\n\n) write to sink, otherwise buffer.
            if (endsWithNewlineNewline(data)) {
              if (buffer.isNotEmpty) {
                buffer.add(data);
                sink.add(buffer.takeBytes());
              } else {
                sink.add(data);
              }
            } else {
              buffer.add(data);
            }
          },
        ),
      );

      return handler.resolve(dio.Response(
        requestOptions: response.requestOptions,
        data: dio.ResponseBody(transformedStream, response.data.contentLength),
        statusCode: response.statusCode,
        headers: response.headers,
      ));
    }

    handler.next(response);
  }

  bool endsWithNewlineNewline(List<int> bytes) {
    if (bytes.length < 2) {
      return false;
    }

    return bytes[bytes.length - 1] == 10 && bytes[bytes.length - 2] == 10;
  }
}
