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
      final queue = TelemetryExportQueue();
      final requests = <ExportRequest>[];
      final serving = serve(queue, requests, 2);
      final telemetry = Telemetry.newPulled(
        config: TelemetryConfig(
          endpoint: 'http://collector/v1/logs',
          headers: {'Authorization': 'Bearer test'},
          resource: [],
          logSeverity: Severity.warn,
        ),
        queue: queue,
      );

      telemetry.emit(
        event: TelemetryEvent(name: 'lk.ping', severity: Severity.info, attributes: []),
      );
      telemetry.recordStats(
        sample: RtcStatsSample(
          trackSid: 'TR_1',
          kind: TrackKind.audio,
          direction: StreamDirection.inbound,
          bytes: 42,
        ),
      );
      await telemetry.flush();
      expect(requests, hasLength(1));
      expect(requests.single.url, 'http://collector/v1/logs');
      expect(requests.single.headers['Content-Type'], 'application/x-protobuf');
      expect(requests.single.headers['Authorization'], 'Bearer test');
      expect(requests.single.body, isNotEmpty);
      expect(telemetry.stats().uploadsSent, 1);

      // Shutdown closes the open stats window, which ships as a second batch.
      await telemetry.shutdown();
      await serving;
      expect(requests, hasLength(2));
      expect(telemetry.stats().dropped, 0);
    });

    test('refuses to start without any transport', () {
      expect(
        () => Telemetry(
          config: TelemetryConfig(endpoint: 'http://collector/v1/logs', headers: {}, resource: [], logSeverity: Severity.warn),
          transport: null,
        ),
        throwsA(anything),
      );
    });
  });
}
