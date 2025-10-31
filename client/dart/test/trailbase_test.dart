import 'dart:async';
import 'dart:io';
import 'dart:convert';
import 'dart:math';

import 'package:trailbase/trailbase.dart';
import 'package:test/test.dart';
import 'package:http/http.dart' as http;

const port = 4006;
const address = '127.0.0.1:${port}';

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

class Author {
  final String id;
  final String user;
  final String name;

  Author.fromJson(Map<String, dynamic> json)
      : id = json['id'],
        user = json['user'],
        name = json['name'];

  @override
  bool operator ==(Object rhs) {
    return rhs is Author &&
        rhs.id == id &&
        rhs.user == user &&
        rhs.name == name;
  }

  @override
  int get hashCode {
    return Object.hash(id, user, name);
  }

  @override
  String toString() {
    return 'Author(${id}, ${user}, ${name})';
  }
}

class Post {
  final String id;
  final String author;
  final String title;
  final String body;

  Post.fromJson(Map<String, dynamic> json)
      : id = json['id'],
        author = json['author'],
        title = json['title'],
        body = json['body'];

  @override
  bool operator ==(Object rhs) {
    return rhs is Post &&
        rhs.id == id &&
        rhs.author == author &&
        rhs.title == title &&
        rhs.body == body;
  }

  @override
  int get hashCode {
    return Object.hash(id, author, title, body);
  }
}

class Comment {
  final int id;
  final String body;
  final (String, Post?) post;
  final (String, Author?) author;

  Comment({
    required this.id,
    required this.body,
    required String postId,
    Map<String, dynamic>? post,
    required String authorId,
    Map<String, dynamic>? authorProfile,
  })  : post = (postId, post != null ? Post.fromJson(post) : null),
        author = (
          authorId,
          authorProfile != null ? Author.fromJson(authorProfile) : null
        );

  Comment.fromJson(Map<String, dynamic> json)
      : this(
          id: json['id'],
          body: json['body'],
          postId: json['post']['id'],
          post: json['post']['data'],
          authorId: json['author']['id'],
          authorProfile: json['author']['data'],
        );

  @override
  bool operator ==(Object rhs) {
    return rhs is Comment &&
        rhs.id == id &&
        rhs.body == body &&
        rhs.post == post &&
        rhs.author == author;
  }

  @override
  int get hashCode {
    return Object.hash(id, body, post, author);
  }

  @override
  String toString() => 'Comment(${id}, ${body}, ${post}, ${author})';
}

Future<Client> connect() async {
  final client = Client('http://${address}');
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
    '--address=${address}',
    // We want at least some parallelism to experience isolate-local state.
    '--runtime-threads=2',
  ]);

  final uri = Uri.parse('http://${address}/api/healthcheck');
  for (int i = 0; i < 100; ++i) {
    try {
      final response = await http.get(uri);
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

Future<List<Event>> take(Stream<Event> stream, int n) {
  const timeout = Duration(seconds: 10);

  return stream.take(n).timeout(timeout, onTimeout: (EventSink<Event> sink) {
    sink.close();
    throw Exception('Failed to take ${n} events from stream');
  }).toList();
}

Future<void> main() async {
  if (!Directory.current.path.endsWith('dart')) {
    throw Exception('Unexpected working directory');
  }

  // If port is set to 4000, we consider this an externally brought up instance.
  if (port != 4000) {
    final process = await initTrailBase();

    tearDownAll(() async {
      process.kill(ProcessSignal.sigkill);
      final _ = await process.exitCode;

      // await process.stderr.forEach(stdout.add);
      // await process.stdout.forEach(stdout.add);
    });
  }

  test('filter', () {
    final params = <String, String>{};
    final filters = [
      Filter(column: 'col0', value: '0', op: CompareOp.greaterThan),
      Filter(column: 'col0', value: '5', op: CompareOp.lessThan),
    ];

    for (final filter in filters) {
      addFiltersToParams(params, 'filter', filter);
    }

    expect(params.length, 2);
  });

  group('client tests', () {
    test('auth', () async {
      final client = await connect();

      final oldTokens = client.tokens();
      expect(oldTokens, isNotNull);

      final user = client.user()!;
      expect(user.id, isNot(equals('')));
      expect(user.email, equals('admin@localhost'));

      await client.logout();
      expect(client.tokens(), isNull);

      // We need to wait a little to push the expiry time in seconds to avoid just getting the same token minted again.
      await Future.delayed(Duration(milliseconds: 1500));

      final newTokens = await client.login('admin@localhost', 'secret');
      expect(newTokens, isNotNull);

      expect(newTokens, isNot(equals(oldTokens)));

      await client.refreshAuthToken();
      expect(newTokens, equals(client.tokens()));
    });

    test('records', () async {
      final client = await connect();
      final api = client.records('simple_strict_table');

      final int now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final sortedMessages = [
        'dart client test 0: =?&${now}',
        'dart client test 1: =?&${now}',
        'dart client test 2: =?&${now}',
      ];
      // NOTE: we're randomizing the lexicographical order of the messages to
      // make sure we're not just sorting by insertion order later on in the
      // test.
      final messages = [...sortedMessages]..shuffle(Random());

      final ids = [];
      for (final msg in messages) {
        ids.add(await api.create({'text_not_null': msg}));
      }

      {
        // Bulk
        final ids = await api.createBulk([
          {'text_not_null': 'dart 1st bulk'},
          {'text_not_null': 'dart 2nd bulk'},
        ]);
        expect(ids.length, 2);
      }

      {
        final response = await api.list(
          filters: [Filter(column: 'text_not_null', value: messages[0])],
        );
        expect(response.records.length, 1);
        expect(response.records[0]['text_not_null'], messages[0]);
      }

      {
        final recordsAsc = (await api.list(
          order: ['+text_not_null'],
          filters: [
            Filter(
                column: 'text_not_null',
                op: CompareOp.like,
                value: '% =?&${now}')
          ],
        ))
            .records;
        expect(recordsAsc.map((el) => el['text_not_null']),
            orderedEquals(sortedMessages));

        final recordsDesc = (await api.list(
          order: ['-text_not_null'],
          filters: [
            Filter(
                column: 'text_not_null', op: CompareOp.like, value: '%${now}')
          ],
        ))
            .records;
        expect(recordsDesc.map((el) => el['text_not_null']).toList().reversed,
            orderedEquals(sortedMessages));
      }

      {
        final response = (await api.list(
          pagination: Pagination(limit: 1),
          order: ['-text_not_null'],
          filters: [
            Filter(
                column: 'text_not_null', op: CompareOp.like, value: '%${now}')
          ],
          count: true,
        ));

        expect(response.totalCount ?? -1, 3);
        expect(response.records[0]['text_not_null'], sortedMessages.last);
        // Ensure there's no extra field, i.e the count doesn't get serialized.
        expect(response.records[0].keys.length, 13);
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

    test('expand foreign records', () async {
      final client = await connect();
      final api = client.records('comment');

      {
        final comment = Comment.fromJson(await api.read(RecordId.integer(1)));

        expect(comment.id, equals(1));
        expect(comment.body, equals('first comment'));
        expect(comment.author.$2, isNull);
        expect(comment.post.$2?.title, isNull);
      }

      {
        final comment = Comment.fromJson(await api.read(
          RecordId.integer(1),
          expand: ['post'],
        ));

        expect(comment.id, equals(1));
        expect(comment.body, equals('first comment'));
        expect(comment.author.$2, isNull);
        expect(comment.post.$2?.title, equals('first post'));
      }

      {
        final response = await api.list(
          expand: ['author', 'post'],
          order: ['-id'],
          pagination: Pagination(limit: 2),
        );

        expect(response.records.length, equals(2));
        final first = Comment.fromJson(response.records[0]);
        expect(first.id, equals(2));
        expect(first.body, equals('second comment'));
        expect(first.author.$2?.name, equals('SecondUser'));
        expect(first.post.$2?.title, equals('first post'));

        final second = Comment.fromJson(response.records[1]);

        final offsetResponse = await api.list(
          expand: ['author', 'post'],
          order: ['-id'],
          pagination: Pagination(limit: 1, offset: 1),
        );

        expect(offsetResponse.records.length, equals(1));
        expect(Comment.fromJson(offsetResponse.records[0]), equals(second));
      }
    });

    test('realtime table and record subscriptions', () async {
      final client = await connect();
      final api = client.records('simple_strict_table');

      final tableEvents = await api.subscribeAll();

      final int now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final createMessage = 'dart client realtime test 0: =?&${now}';
      final id = await api.create({'text_not_null': createMessage});

      final events = await api.subscribe(id);
      {
        // Should not trigger new subscriptions but rather join in the cached broadcast.
        final _ = await api.subscribe(id);
      }

      final updatedMessage = 'dart client updated realtime test 0: ${now}';
      await api.update(id, {'text_not_null': updatedMessage});
      await api.delete(id);

      expect(client.cache.length, equals(2));

      final eventList = await take(events, 2);

      expect(eventList.length, equals(2));
      expect(eventList[0].runtimeType, equals(UpdateEvent));
      expect(
          SimpleStrict.fromJson(eventList[0].value!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: updatedMessage,
          ));

      {
        await Future.delayed(const Duration());
        expect(client.cache.length, equals(1), reason: '${client.cache}');
      }

      expect(eventList[1].runtimeType, equals(DeleteEvent));
      expect(
          SimpleStrict.fromJson(eventList[1].value!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: updatedMessage,
          ));

      final tableEventList = await take(tableEvents, 3);
      expect(tableEventList.length, equals(3));

      expect(tableEventList[0].runtimeType, equals(InsertEvent));
      expect(
          SimpleStrict.fromJson(tableEventList[0].value!),
          SimpleStrict(
            id: id.toString(),
            textNotNull: createMessage,
          ));

      {
        await Future.delayed(const Duration());
        expect(client.cache.length, equals(0), reason: '${client.cache}');
      }
    });

    test('subscription filter', () async {
      final client = await connect();
      final api = client.records('simple_strict_table');

      final int now = DateTime.now().millisecondsSinceEpoch ~/ 1000;
      final updatedMessage = 'dart client updated realtime test 42: ${now}';

      final tableEvents0 = await api.subscribeAll(
          filters: [Filter(column: 'text_not_null', value: updatedMessage)]);

      // Test that filter-values are parsed correctly to column type.
      final tableEvents1 = await api.subscribeAll(filters: [
        Filter(column: 'text_not_null', op: CompareOp.greaterThan, value: '0')
      ]);

      final createMessage = 'dart client realtime test 42: =?&${now}';
      final id = await api.create({'text_not_null': createMessage});

      await api.update(id, {'text_not_null': updatedMessage});
      await api.delete(id);

      {
        final eventList = await take(tableEvents0, 2);

        expect(eventList.length, equals(2));
        expect(eventList[0].runtimeType, equals(UpdateEvent));
        expect(
            SimpleStrict.fromJson(eventList[0].value!),
            SimpleStrict(
              id: id.toString(),
              textNotNull: updatedMessage,
            ));

        // Demonstrate pattern-mathching/destructuring.
        if (eventList[1] case DeleteEvent(value: final v)) {
          expect(
              SimpleStrict.fromJson(v),
              SimpleStrict(
                id: id.toString(),
                textNotNull: updatedMessage,
              ));
        } else {
          throw ArgumentError.value(
              'expected DeleteEvent, got ${eventList[1]}');
        }
      }

      {
        final eventList = await take(tableEvents1, 3);
        expect(eventList.length, equals(3));
        expect(eventList[0].runtimeType, equals(InsertEvent));
      }
    });

    test('custom schema', () async {
      final client = await connect();
      final api = client.records('simple_schema_table');

      await api.create({
        'data': '{ "name": "Eve" }',
      });

      await api.create({
        'data': {'name': 'Eve'},
      });

      // Test that invalid input produces a client-error, i.e. 400.
      expect(() async {
        await api.create({
          'data': "{ 4: 'Eve' }",
        });
      }, throwsA(predicate((e) {
        if (e is HttpException) {
          return e.status == 400;
        }
        return false;
      })));
    });
  });
}
