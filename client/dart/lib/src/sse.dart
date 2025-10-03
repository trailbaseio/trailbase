import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

class HttpException implements Exception {
  final int status;
  final String? message;

  const HttpException(this.status, this.message);

  @override
  String toString() => 'HttpException(${status}, msg=${message})';
}

sealed class Event {
  Map<String, dynamic>? get value;
}

class InsertEvent extends Event {
  @override
  final Map<String, dynamic> value;

  InsertEvent(this.value);

  @override
  String toString() => 'InsertEvent(${value})';
}

class UpdateEvent extends Event {
  @override
  final Map<String, dynamic> value;

  UpdateEvent(this.value);

  @override
  String toString() => 'UpdateEvent(${value})';
}

class DeleteEvent extends Event {
  @override
  final Map<String, dynamic> value;

  DeleteEvent(this.value);

  @override
  String toString() => 'DeleteEvent(${value})';
}

class ErrorEvent extends Event {
  final String _error;

  ErrorEvent(this._error);

  @override
  Map<String, dynamic>? get value => null;

  @override
  String toString() => 'ErrorEvent(${_error})';
}

Future<Stream<Event>> connectSse(
  http.Client client,
  http.Request request, {
  Map<String, Future<StreamController<Event>>>? cache,
}) async {
  final key = request.url.toString();

  Future<StreamController<Event>> connectSseImpl() async {
    final response = await client.send(request);
    if (response.statusCode != 200) {
      throw HttpException(response.statusCode, response.toString());
    }

    late final StreamController<Event> ctrl;
    StreamSubscription<List<int>>? subscription;

    ctrl = StreamController<Event>.broadcast(
      onCancel: () {
        subscription?.cancel();
        cache?.remove(key);
      },
      onListen: () {
        // NOTE: Unlike the default StreamController, the broadcast one doesn't
        // buffer. Hence we have to delay listening until somebody starts
        // listening.
        final buffer = BytesBuilder();
        subscription = response.stream.listen(
          (List<int> data) {
            if (_endsWithNewlineNewline(data)) {
              if (buffer.isNotEmpty) {
                buffer.add(data);

                final event = _decodeEvent(buffer.takeBytes());
                if (event != null) ctrl.add(event);
              } else {
                final event = _decodeEvent(data);
                if (event != null) ctrl.add(event);
              }
            } else {
              buffer.add(data);
            }
          },
          onDone: () => ctrl.close(),
          onError: (error) => ctrl.addError(error),
          cancelOnError: true,
        );
      },
    );

    return ctrl;
  }

  if (cache != null) {
    final ctrl = cache.putIfAbsent(key, () => connectSseImpl());
    return (await ctrl).stream;
  }

  return (await connectSseImpl()).stream;
}

bool _endsWithNewlineNewline(List<int> bytes) {
  if (bytes.length >= 2) {
    return bytes[bytes.length - 1] == 10 && bytes[bytes.length - 2] == 10;
  }
  return false;
}

Event _eventfromJson(Map<String, dynamic> json) {
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

Event? _decodeEvent(List<int> bytes) {
  final decoded = utf8.decode(bytes);
  if (decoded.startsWith('data: ')) {
    // Cut off "data: " and decode.
    return _eventfromJson(jsonDecode(decoded.substring(6)));
  }

  // Heart-beat, do nothing.
  return null;
}
