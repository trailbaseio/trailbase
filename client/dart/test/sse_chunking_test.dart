import 'dart:async';
import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:test/test.dart';
import 'package:trailbase/src/sse.dart';
import 'package:trailbase/src/transport.dart';

class MockTransport implements Transport {
  final List<int> payload;

  MockTransport(this.payload);

  @override
  Future<http.Response> fetch(
    String path, {
    Method method = Method.get,
    Map<String, String>? headers,
    String? body,
    Map<String, dynamic>? queryParams,
  }) async {
    throw UnimplementedError();
  }

  @override
  Future<http.StreamedResponse> stream(
    Uri uri, {
    Map<String, String>? headers,
  }) async {
    // Create a stream that emits our payload in a single chunk
    final stream = Stream<List<int>>.fromIterable([payload]);
    
    return http.StreamedResponse(
      stream,
      200,
      request: http.Request('GET', uri),
    );
  }
}

void main() {
  test('connectSse properly splits chunked events', () async {
    // Create a scenario where TWO SSE events arrive perfectly merged inside a single TCP buffer chunk
    const event1 = '{"Insert": {"id": 1, "test": "a"}, "seq": 1}';
    const event2 = '{"Insert": {"id": 2, "test": "b"}, "seq": 2}';
    
    // The server separates events with \n\n.
    // If the network delivers them in a single chunk, it looks like this:
    const payload = 'data: $event1\n\n'
                    'data: $event2\n\n';
                    
    final transport = MockTransport(utf8.encode(payload));
    
    final sseStream = await connectSse(transport, Uri.parse('http://localhost'));
    
    final events = await sseStream.take(2).toList();
    
    expect(events.length, 2);
    
    expect(events[0], isA<InsertEvent>());
    expect(events[0].seq, 1);
    
    expect(events[1], isA<InsertEvent>());
    expect(events[1].seq, 2);
  });
}
