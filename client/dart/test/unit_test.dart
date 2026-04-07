import 'dart:convert';

import 'package:test/test.dart';
import 'package:trailbase/trailbase.dart';

Future<void> main() async {
  test('SSE Error Event decoding', () {
    const json0 = '''
        {
          "Error": {
            "status": 1,
            "message": "test"
          },
          "seq": 3
        }''';

    final ev0 = decodeEvent(utf8.encode('data: ${json0}'));

    expect(ev0, isNotNull);
    expect(3, equals(ev0!.seq));
    final errEv = ev0 as ErrorEvent;
    expect(errEv.status, equals(ErrorEvent.statusForbidden));
    expect(errEv.message, equals('test'));

    const json1 = '''
        {
          "Error": {
            "status": 1
          }
        }''';

    final ev1 = decodeEvent(utf8.encode('data: ${json1}'));

    expect(ev1, isNotNull);
    final errEv1 = ev1 as ErrorEvent;
    expect(errEv1.seq, equals(null));
    expect(errEv1.status, equals(ErrorEvent.statusForbidden));
    expect(errEv1.message, equals(null));
  });

  test('SSE Update Event decoding', () {
    const json = '''
        {
          "Update": {
            "col0": "val0",
            "col1": 4
          },
          "seq": 4
        }''';

    final ev0 = decodeEvent(utf8.encode('data: ${json}'));

    expect(ev0, isNotNull);
    final errEv = ev0 as UpdateEvent;
    expect(errEv.seq, equals(4));
    expect(
        errEv.value,
        equals({
          'col0': 'val0',
          'col1': 4,
        }));
  });
}
