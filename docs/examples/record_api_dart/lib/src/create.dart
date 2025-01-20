import 'package:trailbase/trailbase.dart';

Future<RecordId> create(Client client) async => await client
    .records('simple_strict_table')
    .create({'text_not_null': 'test'});
