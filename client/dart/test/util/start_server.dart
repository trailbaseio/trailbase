import 'dart:async';
import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:logging/logging.dart';

Future<Process> startServer({
  required int port,
  Duration timeout = const Duration(minutes: 10),
}) async {
  final result = await Process.run('python3', ['../runner.py', 'build']);
  if (result.exitCode > 0) {
    throw Exception(
        'Cargo build failed.\n\nstdout: ${result.stdout}\n\nstderr: ${result.stderr}\n');
  }

  final process = await Process.start('python3', [
    '../runner.py',
    'run',
    '--port=${port}',
    // We want at least some parallelism to experience isolate-local state.
    '--runtime-threads=2',
  ]);

  process.stderr.listen(stderr.add);
  process.stdout.listen(stdout.add);

  final address = '127.0.0.1:${port}';
  final healthUri = Uri.parse('http://${address}/api/healthcheck');

  Future<int> exitCode() async {
    return await process.exitCode
        .timeout(Duration(milliseconds: 50), onTimeout: () => -1);
  }

  final started = DateTime.now();
  while (true) {
    final code = await exitCode();
    if (code > 0) {
      throw Exception('server exited abnormally with code: ${code}.');
    }

    try {
      final response = await http.get(healthUri);
      if (response.statusCode == HttpStatus.ok) {
        return process;
      }
    } on SocketException catch (err) {
      _logger.fine('Failed to call ${healthUri}: ${err}');
    } catch (err) {
      _logger.info('Health check failed: ${err}');
    }

    if (DateTime.now().difference(started) > timeout) {
      process.kill(ProcessSignal.sigkill);
      final code = await exitCode();

      throw Exception('Starting Trailbase within ${timeout} failed: ${code}.');
    }

    await Future.delayed(Duration(microseconds: 500));
  }
}

final _logger = Logger('test');
