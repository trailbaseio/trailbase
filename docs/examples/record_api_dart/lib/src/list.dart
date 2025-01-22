import 'package:trailbase/trailbase.dart';

Future<ListResponse> list(Client client) async =>
    await client.records('movies').list(
      pagination: Pagination(limit: 3),
      order: ['rank'],
      filters: ['watch_time[lt]=120', 'description[like]=%love%'],
    );
