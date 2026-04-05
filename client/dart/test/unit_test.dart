import 'dart:convert';

import 'package:test/test.dart';
import 'package:trailbase/trailbase.dart';

Future<void> main() async {
  test('SSE Event decoding', () {
    const json = '''
        {
          "Error": {
            "status": 1,
            "message": "test"
          },
          "seq": 3
         }''';

    final ev0 = decodeEvent(utf8.encode('data: ${json}'));

    expect(ev0, isNotNull);
    expect(3, equals(ev0!.seq));
    final errEv = ev0 as ErrorEvent;
    expect(errEv.status, equals(ErrorEvent.statusForbidden));
    expect(errEv.message, equals('test'));
  });
}
