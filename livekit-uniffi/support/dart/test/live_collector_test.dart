import 'dart:io';
import 'dart:typed_data';

import 'package:http/http.dart' as http;
import 'package:livekit_uniffi/livekit_telemetry.dart';
import 'package:livekit_uniffi/livekit_uniffi.dart';
import 'package:test/test.dart';

/// Posts one custom event to a real collector through the pull queue.
///
/// Skipped unless a collector is named: `LK_TELEMETRY_ENDPOINT` (a local OpenTelemetry collector,
/// e.g. `http://localhost:4318` — read by the Rust core itself, the test-only override), or
/// `LK_TELEMETRY_URL` + `LK_TELEMETRY_TOKEN` (a LiveKit Cloud project and a token carrying the
/// observability grant). Unlike `telemetry_test.dart`, which fakes the collector's answer, this
/// one performs the request so the Dart binding is exercised against a real ingest.
///
/// Dart drives the transport rather than implementing `TelemetryTransport`: uniffi-dart's
/// foreign-trait callbacks are isolate-bound, so Rust calling into Dart from a tokio worker aborts
/// the VM. The queue inverts that — Dart awaits `next()` and reports the answer with `complete()`.
Future<void> serve(TelemetryExportQueue queue, List<int> statuses, int count) async {
  final client = http.Client();
  try {
    for (var i = 0; i < count; i++) {
      final pending = await queue.next();
      if (pending == null) return;
      final request = pending.request;
      try {
        final response = await client.post(
          Uri.parse(request.url),
          headers: request.headers,
          body: request.body,
        );
        statuses.add(response.statusCode);
        queue.complete(
          id: pending.id,
          response: ExportResponse(
            status: response.statusCode,
            headers: {},
            body: Uint8List.fromList(response.bodyBytes),
          ),
        );
      } catch (error) {
        statuses.add(-1);
        queue.fail(
          id: pending.id,
          error: RetryableExportException(reason: '$error', retryAfterMs: null),
        );
      }
    }
  } finally {
    client.close();
  }
}

void main() {
  final endpoint = Platform.environment['LK_TELEMETRY_ENDPOINT'];
  final url = Platform.environment['LK_TELEMETRY_URL'];
  final token = Platform.environment['LK_TELEMETRY_TOKEN'];
  final target = endpoint ?? url;

  test('posts a custom event to a live collector through the pull queue', () async {
    final statuses = <int>[];
    final queue = telemetryConfigurePulled(
      config: TelemetryConfig(
        resource: [
          Attribute(key: 'service.name', value: StrAttributeValue('livekit-client-dart')),
          Attribute(key: 'service.version', value: StrAttributeValue('0.0.0-local')),
          Attribute(key: 'os.name', value: StrAttributeValue(Platform.operatingSystem)),
        ],
        logSeverity: Severity.warn,
      ),
      instruments: [],
    );
    final serving = serve(queue, statuses, 2);

    final room = telemetryScope()!;
    room.setServer(url: url ?? 'ws://localhost:7880', token: token ?? 'local');
    room.emitCustom(name: 'ping', attributes: {'seq': '1'});
    await telemetryFlush();
    await telemetryShutdown();
    await serving;
    queue.finish();

    print('dart → $target: statuses $statuses');
    expect(statuses, isNotEmpty);
    expect(statuses.first, inInclusiveRange(200, 299));
  }, skip: target == null ? 'set LK_TELEMETRY_ENDPOINT, or LK_TELEMETRY_URL and LK_TELEMETRY_TOKEN' : null);
}
