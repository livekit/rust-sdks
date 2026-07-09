import 'package:test/test.dart';
import 'package:livekit_uniffi/livekit_uniffi.dart';

void main() {
  // Smoke test for the whole FFI path: the build hook resolves the native
  // library, Dart calls a Rust function, and the result crosses back.
  test('buildVersion returns a non-empty version string', () {
    expect(buildVersion(), isNotEmpty);
  });
}
