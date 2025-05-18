import 'package:trailbase/trailbase.dart';

Future<ListResponse> list(Client client) async =>
    await client.records('movies').list(
      pagination: Pagination(limit: 3),
      order: ['rank'],
      filters: [
        Filter(
          column: 'watch_time',
          op: CompareOp.lessThan,
          value: '120',
        ),
        Filter(
          column: 'description',
          op: CompareOp.like,
          value: '%love%',
        ),
      ],
    );
