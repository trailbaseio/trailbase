import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:http/http.dart' as http;

abstract class Event {
  Event();

  Map<String, dynamic>? value();

  static Event fromJson(Map<String, dynamic> json) {
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
}

class InsertEvent extends Event {
  final Map<String, dynamic>? _value;

  InsertEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'InsertEvent(${_value})';
}

class UpdateEvent extends Event {
  final Map<String, dynamic>? _value;

  UpdateEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'UpdateEvent(${_value})';
}

class DeleteEvent extends Event {
  final Map<String, dynamic>? _value;

  DeleteEvent(this._value);

  @override
  Map<String, dynamic>? value() => _value;

  @override
  String toString() => 'DeleteEvent(${_value})';
}

class ErrorEvent extends Event {
  final String _error;

  ErrorEvent(this._error);

  @override
  Map<String, dynamic>? value() => null;

  @override
  String toString() => 'ErrorEvent(${_error})';
}

Future<Stream<Event>> connectSse(
  http.Client client,
  http.Request request,
) async {
  // NOTE: We could use a `StreamController.broadcast()` and track existing
  // streams keyed by `uri` to merge multiple concurrent subscriptions.
  final response = await client.send(request);
  if (response.statusCode != 200) {
    throw Exception('[${response.statusCode}] ${response}');
  }

  final buffer = BytesBuilder();
  final sink = StreamController<Uint8List>();

  response.stream.listen(
    (List<int> data) {
      if (_endsWithNewlineNewline(data)) {
        if (buffer.isNotEmpty) {
          buffer.add(data);
          sink.add(buffer.takeBytes());
        } else {
          sink.add(Uint8List.fromList(data));
        }
      } else {
        buffer.add(data);
      }
    },
    onDone: () => sink.close(),
    onError: (error) => sink.addError(error),
    cancelOnError: true,
  );

  return sink.stream.expand(_decodeEvent);
}

bool _endsWithNewlineNewline(List<int> bytes) {
  if (bytes.length >= 2) {
    return bytes[bytes.length - 1] == 10 && bytes[bytes.length - 2] == 10;
  }
  return false;
}

List<Event> _decodeEvent(Uint8List bytes) {
  final decoded = utf8.decode(bytes);
  if (decoded.startsWith('data: ')) {
    // Cut off "data: " and decode.
    return [Event.fromJson(jsonDecode(decoded.substring(6)))];
  }

  // Heart-beat, do nothing.
  return [];
}
