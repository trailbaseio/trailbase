import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:meta/meta.dart';

import './transport.dart';

sealed class Event {
  final int? seq;

  const Event(this.seq);

  Map<String, dynamic>? get value;
}

class InsertEvent extends Event {
  @override
  final Map<String, dynamic> value;

  InsertEvent(super.seq, this.value);

  @override
  String toString() => 'InsertEvent(${value})';
}

class UpdateEvent extends Event {
  @override
  final Map<String, dynamic> value;

  UpdateEvent(super.seq, this.value);

  @override
  String toString() => 'UpdateEvent(${value})';
}

class DeleteEvent extends Event {
  @override
  final Map<String, dynamic> value;

  DeleteEvent(super.seq, this.value);

  @override
  String toString() => 'DeleteEvent(${value})';
}

class ErrorEvent extends Event {
  final int _status;
  final String? _message;

  static const int statusUnknown = 0;
  static const int statusForbidden = 1;
  static const int statusEventLoss = 2;

  ErrorEvent(super.seq, this._status, this._message);

  @override
  Map<String, dynamic>? get value => null;

  int get status => _status;
  String? get message => _message;

  @override
  String toString() => 'ErrorEvent(${_status}, ${_message})';
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

                final event = decodeEvent(buffer.takeBytes());
                if (event != null) ctrl.add(event);
              } else {
                final event = decodeEvent(data);
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
  final seq = json['seq'] as int?;
  final insert = json['Insert'];
  if (insert != null) {
    return InsertEvent(seq, insert as Map<String, dynamic>);
  }

  final update = json['Update'];
  if (update != null) {
    return UpdateEvent(seq, update as Map<String, dynamic>);
  }

  final delete = json['Delete'];
  if (delete != null) {
    return DeleteEvent(seq, delete as Map<String, dynamic>);
  }

  final error = json['Error'];
  if (error != null) {
    return ErrorEvent(
      seq,
      (error['status'] as int?) ?? ErrorEvent.statusUnknown,
      error['message'] as String?,
    );
  }

  throw Exception('Failed to parse event: ${json}');
}

@visibleForTesting
Event? decodeEvent(List<int> bytes) {
  final decoded = utf8.decode(bytes);
  if (decoded.startsWith('data: ')) {
    // Cut off "data: " and decode.
    return _eventfromJson(jsonDecode(decoded.substring(6)));
  }

  // Heart-beat, do nothing.
  return null;
}
