import 'package:test/test.dart';
import 'package:trailbase/trailbase.dart';

import 'package:record_api/record_api.dart';

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

  Map<String, dynamic> toJson() => {
        'id': id,
        'text_null': textNull,
        'text_default': textDefault,
        'text_not_null': textNotNull,
      };
}

Future<Client> connect() async {
  final client = Client('http://localhost:4000');
  await client.login('admin@localhost', 'secret');
  return client;
}

void main() {
  test('Test code examples', () async {
      final client = await connect();

      final tableStream = await subscribeAll(client);

      final id = await create(client);

      final recordStream = await subscribe(client, id);

      {
        final json = await read(client, id);
        final record = SimpleStrict.fromJson(json);
        expect(record.textNotNull, equals('test'));
      }

      {
        await update(client, id);
        final json = await read(client, id);
        final record = SimpleStrict.fromJson(json);
        expect(record.textNotNull, equals('updated'));
      }

      await delete(client, id);

      expect(await recordStream.length, equals(2));

      final tableEventList =
          await tableStream.timeout(Duration(seconds: 5), onTimeout: (sink) {
        print('Stream timeout');
        sink.close();
      }).toList();
      expect(tableEventList.length, equals(3));
  });
}
