import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import './transport.dart';

sealed class Event {
  int? get seq;
  Map<String, dynamic>? get value;
}

class InsertEvent implements Event {
  @override
  final int? seq;
  @override
  final Map<String, dynamic> value;

  InsertEvent(this.seq, this.value);

  @override
  String toString() => 'InsertEvent(${value})';
}

class UpdateEvent implements Event {
  @override
  final int? seq;
  @override
  final Map<String, dynamic> value;

  UpdateEvent(this.seq, this.value);

  @override
  String toString() => 'UpdateEvent(${value})';
}

class DeleteEvent extends Event {
  @override
  final int? seq;
  @override
  final Map<String, dynamic> value;

  DeleteEvent(this.seq, this.value);

  @override
  String toString() => 'DeleteEvent(${value})';
}

class ErrorEvent extends Event {
  @override
  final int? seq;
  final String error;

  ErrorEvent(this.seq, this.error);

  @override
  Map<String, dynamic>? get value => null;

  @override
  String toString() => 'ErrorEvent(${error})';
}

Future<Stream<Event>> connectSse(
  Transport client,
  Uri uri, {
  Map<String, String>? headers,
  Map<String, Future<StreamController<Event>>>? cache,
}) async {
  final key = uri.toString();

  Future<StreamController<Event>> connectSseImpl() async {
    final response = await client.stream(
      uri,
      headers: headers,
    );

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
  final type = json['type'];
  if (type == null) {
    throw Exception('Unknown event type: ${json}');
  }
  final seq = json['seq'] as int?;

  return switch (type) {
    'insert' => InsertEvent(seq, json['value']),
    'update' => UpdateEvent(seq, json['value']),
    'delete' => DeleteEvent(seq, json['value']),
    'error' => ErrorEvent(seq, json['error'] as String),
    _ => throw Exception('Failed to parse event: ${json}'),
  };
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
