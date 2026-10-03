import 'dart:convert';
import 'dart:typed_data';

import 'package:livekit_uniffi/livekit_telemetry.dart';
import 'package:livekit_uniffi/livekit_uniffi.dart';
import 'package:test/test.dart';

/// Rust must never call back into Dart from its own threads: uniffi-dart's foreign-trait
/// callbacks are isolate-bound (`Pointer.fromFunction`) and the VM aborts with "Cannot invoke
/// native callback outside an isolate" when the exporter invokes `TelemetryTransport.send` from
/// a tokio worker. The pull queue inverts the direction: Dart polls `tryNext()` from its own
/// timer (never leaving a Rust future holding a continuation of an isolate that may die),
/// performs the request, and reports the collector's answer back with `complete()` (or `fail()`
/// when there was none); the core classifies the status and body.
Future<void> serve(TelemetryExportQueue queue, List<ExportRequest> sink, int count) async {
  for (var i = 0; i < count; i++) {
    var pending = queue.tryNext();
    while (pending == null) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
      pending = queue.tryNext();
    }
    sink.add(pending.request);
    queue.complete(id: pending.id, response: ExportResponse(status: 200, headers: {}, body: Uint8List(0)));
  }
}

/// An unsigned participant token with the observability grant: the core reads claims, never
/// verifies them (the collector does).
String grantedToken() {
  String part(Map<String, Object> json) =>
      base64Url.encode(utf8.encode(jsonEncode(json))).replaceAll('=', '');
  final exp = DateTime.now().millisecondsSinceEpoch ~/ 1000 + 3600;
  return '${part({'alg': 'HS256'})}.${part({'exp': exp, 'observability': {'write': true}})}.sig';
}

void main() {
  group('telemetry', () {
    test('exports a room\'s records to its project through the pull queue', () async {
      final requests = <ExportRequest>[];
      final queue = telemetryConfigurePulled(
        config: TelemetryConfig(resource: [], logSeverity: Severity.warn),
        instruments: [],
      );
      final serving = serve(queue, requests, 2);

      final room = telemetryScope()!;
      final token = grantedToken();
      room.setServer(url: 'wss://my-project.livekit.cloud', token: token);
      room.setAttribute(key: 'app.call_id', value: 'c1');
      room.emitCustom(name: 'checkout', attributes: {'plan': 'pro'});
      await telemetryFlush();
      expect(requests, hasLength(1));
      expect(requests.single.url, 'https://my-project.livekit.cloud/observability/client/logs/otlp/v0');
      expect(requests.single.headers['Authorization'], 'Bearer $token');
      expect(requests.single.headers['Content-Type'], 'application/x-protobuf');
      expect(requests.single.body, isNotEmpty);
      expect(telemetryStats()!.uploadsSent, 1);
      expect(telemetryStats()!.dropped, 0);
      expect(room.statsPollIntervalMs(), 30000);

      // Shutdown leaves the session summary, a process-level batch of its own.
      await telemetryShutdown();
      await serving;
      expect(requests, hasLength(2));
      expect(telemetryStats(), isNull);
      // The queue outlives the pipeline (Dart holds it): finishing it ends the serving loop.
      queue.finish();
      expect(await queue.next(), isNull);
    });

    test('refuses to start without any transport', () {
      expect(
        () => telemetryConfigure(
          config: TelemetryConfig(resource: [], logSeverity: Severity.warn),
          transport: null,
          instruments: [],
        ),
        throwsA(anything),
      );
    });
  });
}
