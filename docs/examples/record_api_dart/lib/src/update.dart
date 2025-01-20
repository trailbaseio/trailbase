import 'package:trailbase/trailbase.dart';

Future<void> update(Client client, RecordId id) async => await client
    .records('simple_strict_table')
    .update(id, {'text_not_null': 'updated'});
