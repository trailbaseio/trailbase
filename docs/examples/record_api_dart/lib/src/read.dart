import 'package:trailbase/trailbase.dart';

Future<Map<String, dynamic>> read(Client client, RecordId id) async =>
    await client.records('simple_strict_table').read(id);
