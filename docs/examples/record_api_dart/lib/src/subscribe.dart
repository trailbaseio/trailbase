import 'dart:async';

import 'package:trailbase/trailbase.dart';

Future<Stream<Event>> subscribe(Client client, RecordId id) async =>
    await client.records('simple_strict_table').subscribe(id);

Future<Stream<Event>> subscribeAll(Client client) async =>
    await client.records('simple_strict_table').subscribeAll();
