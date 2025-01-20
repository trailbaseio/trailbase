import 'package:trailbase/trailbase.dart';

Future<void> delete(Client client, RecordId id) async =>
    await client.records('simple_strict_table').delete(id);
