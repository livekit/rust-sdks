import 'package:livekit_uniffi/livekit_telemetry.dart';
import 'package:livekit_uniffi/livekit_uniffi.dart';
import 'package:test/test.dart';

/// Rust must never call back into Dart from its own threads: uniffi-dart's foreign-trait
/// callbacks are isolate-bound (`Pointer.fromFunction`) and the VM aborts with "Cannot invoke
/// native callback outside an isolate" when the exporter invokes `TelemetryTransport.send` from
/// a tokio worker. The pull queue inverts the direction: Dart awaits `next()`, performs the
/// request, and reports back with `complete()`.
Future<void> serve(TelemetryExportQueue queue, List<ExportRequest> sink, int count) async {
  for (var i = 0; i < count; i++) {
    final pending = await queue.next();
    if (pending == null) return;
    sink.add(pending.request);
    queue.complete(id: pending.id, error: null);
  }
}

void main() {
  group('telemetry', () {
    test('exports through the pull queue from the Dart side', () async {
      final requests = <ExportRequest>[];
      final queue = telemetryConfigurePulled(
        config: TelemetryConfig(
          endpoint: 'http://collector/v1/logs',
          headers: {'Authorization': 'Bearer test'},
          resource: [],
          logSeverity: Severity.warn,
        ),
      );
      final serving = serve(queue, requests, 2);

      telemetryEmit(
        event: TelemetryEvent(name: 'lk.ping', severity: Severity.info, attributes: []),
      );
      telemetryScope()!.recordStats(
        sample: RtcStatsSample(
          trackSid: 'TR_1',
          kind: TrackKind.audio,
          direction: StreamDirection.inbound,
          bytes: 42,
        ),
      );
      await telemetryFlush();
      expect(requests, hasLength(1));
      expect(requests.single.url, 'http://collector/v1/logs');
      expect(requests.single.headers['Content-Type'], 'application/x-protobuf');
      expect(requests.single.headers['Authorization'], 'Bearer test');
      expect(requests.single.body, isNotEmpty);
      expect(telemetryStats()!.uploadsSent, 1);
      expect(telemetryStats()!.dropped, 0);

      // Shutdown closes the open stats window, which ships as a second batch.
      await telemetryShutdown();
      await serving;
      expect(requests, hasLength(2));
      expect(telemetryStats(), isNull);
    });

    test('refuses to start without any transport', () {
      expect(
        () => telemetryConfigure(
          config: TelemetryConfig(endpoint: 'http://collector/v1/logs', headers: {}, resource: [], logSeverity: Severity.warn),
          transport: null,
        ),
        throwsA(anything),
      );
    });
  });
}
