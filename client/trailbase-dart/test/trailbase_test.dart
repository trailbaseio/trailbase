import 'dart:io';
import 'dart:convert';

import 'package:trailbase/trailbase.dart';
import 'package:test/test.dart';
import 'package:dio/dio.dart';

const port = 4006;

class SimpleStrict {
  final String id;

  final String? textNull;
  final String textDefault;
  final String textNotNull;

  SimpleStrict({
    required this.id,
    this.textNull,
    this.textDefault = '',
    required this.textNotNull,
  });

  SimpleStrict.fromJson(Map<String, dynamic> json)
      : id = json['id'],
        textNull = json['text_null'],
        textDefault = json['text_default'],
        textNotNull = json['text_not_null'];

  @override
  bool operator ==(Object other) {
    return other is SimpleStrict &&
        id == other.id &&
        textNull == other.textNull &&
        textDefault == other.textDefault &&
        textNotNull == other.textNotNull;
  }

  @override
  int get hashCode {
    return Object.hash(id, textNull, textDefault, textNotNull);
  }

  @override
  String toString() =>
      'SimpleStrict(id: ${id}, textNull: ${textNull}, textDefault: ${textDefault}, textNotNull: ${textNotNull})';
}

Future<Client> connect() async {
  final client = Client('http://127.0.0.1:${port}');
  await client.login('admin@localhost', 'secret');
  return client;
}

Future<Process> initTrailBase() async {
  final result = await Process.run('cargo', ['build'],
      stdoutEncoding: utf8, stderrEncoding: utf8);
  if (result.exitCode > 0) {
    throw Exception(
        'Cargo build failed.\n\nstdout: ${result.stdout}\n\nstderr: ${result.stderr}\n');
  }

  // Relative to CWD.
  const depotPath = '../testfixture';

  final process = await Process.start('cargo', [
    'run',
    '--',
    '--data-dir=${depotPath}',
    'run',
    '--address=127.0.0.1:${port}',
    // We want at least some parallelism to experience isolate-local state.
    '--js-runtime-threads=2',
  ]);

  final dio = Dio();
  for (int i = 0; i < 100; ++i) {
    try {
      final response = await dio.fetch(
          RequestOptions(path: 'http://127.0.0.1:${port}/api/healthcheck'));
      if (response.statusCode == 200) {
        return process;
      }
    } catch (err) {
      print('Trying to connect to TrailBase');
    }

    if (await process.exitCode
            .timeout(Duration(milliseconds: 500), onTimeout: () => -1) >=
        0) {
      break;
    }
  }

  process.kill(ProcessSignal.sigkill);
  final exitCode = await process.exitCode;

  await process.stderr.forEach(stdout.add);
  await process.stdout.forEach(stdout.add);

  throw Exception('Cargo run failed: ${exitCode}.');
}

Future<void> main() async {
  if (!Directory.current.path.endsWith('trailbase-dart')) {
    throw Exception('Unexpected working directory');
  }

  final process = await initTrailBase();

  tearDownAll(() async {
    process.kill(ProcessSignal.sigkill);
    final _ = await process.exitCode;

    // await process.stderr.forEach(stdout.add);
    // await process.stdout.forEach(stdout.add);
  });

  group('client tests', () {
    test('auth', () async {
      final client = await connect();

      final oldTokens = client.tokens();
      expect(oldTokens, isNotNull);
      expect(oldTokens!.valid, isTrue);

      final user = client.user()!;
      expect(user.id, isNot(equals('')));
      expect(user.email, equals('admin@localhost'));

      await client.logout();
      expect(client.tokens(), isNull);

      // We need to wait a little to push the expiry time in seconds to avoid just getting the same token minted again.
      await Future.delayed(Duration(milliseconds: 1500));

      final newTokens = await client.login('admin@localhost', 'secret');
      expect(newTokens, isNotNull);
      expect(newTokens.valid, isTrue);

      expect(newTokens, isNot(equals(oldTokens)));

      await client.refreshAuthToken();
      expect(newTokens, equals(client.tokens()));
    });

    test('records', () async {
      final client = await connect();
      final api = client.records('simple_strict_table');

      final int now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final messages = [
        'dart client test 0: =?&${now}',
        'dart client test 1: =?&${now}',
      ];
      final ids = [];
      for (final msg in messages) {
        ids.add(await api.create({'text_not_null': msg}));
      }

      {
        final records = await api.list(
          filters: ['text_not_null=${messages[0]}'],
        );
        expect(records.length, 1);
        expect(records[0]['text_not_null'], messages[0]);
      }

      {
        final recordsAsc = await api.list(
          order: ['+text_not_null'],
          filters: ['text_not_null[like]=% =?&${now}'],
        );
        expect(recordsAsc.map((el) => el['text_not_null']),
            orderedEquals(messages));

        final recordsDesc = await api.list(
          order: ['-text_not_null'],
          filters: ['text_not_null[like]=%${now}'],
        );
        expect(recordsDesc.map((el) => el['text_not_null']).toList().reversed,
            orderedEquals(messages));
      }

      final record = SimpleStrict.fromJson(await api.read(ids[0]));

      expect(ids[0] == record.id, isTrue);
      // Note: the .id() is needed otherwise we call String's operator==. It's not ideal
      // but we didn't come up with a better option.
      expect(record.id.id() == ids[0], isTrue);
      expect(RecordId.uuid(record.id) == ids[0], isTrue);

      expect(record.textNotNull, messages[0]);

      final updatedMessage = 'dart client updated test 0: ${now}';
      await api.update(ids[0], {'text_not_null': updatedMessage});
      final updatedRecord = SimpleStrict.fromJson(await api.read(ids[0]));
      expect(updatedRecord.textNotNull, updatedMessage);

      await api.delete(ids[0]);
      expect(() async => await api.read(ids[0]), throwsException);
    });

    test('realtime', () async {
      final client = await connect();
      final api = client.records('simple_strict_table');

      final tableEvents = await api.subscribeAll();

      final int now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final createMessage = 'dart client realtime test 0: =?&${now}';
      final id = await api.create({'text_not_null': createMessage});

      final events = await api.subscribe(id);

      final updatedMessage = 'dart client updated realtime test 0: ${now}';
      await api.update(id, {'text_not_null': updatedMessage});
      await api.delete(id);

      final eventList =
          await events.timeout(Duration(seconds: 10), onTimeout: (sink) {
        print('Stream timeout');
        sink.close();
      }).toList();

      expect(eventList.length, equals(2));
      expect(eventList[0].runtimeType, equals(UpdateEvent));
      expect(
          SimpleStrict.fromJson(eventList[0].value()!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: updatedMessage,
          ));

      expect(eventList[1].runtimeType, equals(DeleteEvent));
      expect(
          SimpleStrict.fromJson(eventList[1].value()!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: updatedMessage,
          ));

      final tableEventList =
          await tableEvents.timeout(Duration(seconds: 10), onTimeout: (sink) {
        print('Stream timeout');
        sink.close();
      }).toList();
      expect(tableEventList.length, equals(3));

      expect(tableEventList[0].runtimeType, equals(InsertEvent));
      expect(
          SimpleStrict.fromJson(tableEventList[0].value()!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: createMessage,
          ));
    });
  });
}
