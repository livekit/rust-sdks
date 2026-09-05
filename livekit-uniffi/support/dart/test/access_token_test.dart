import 'package:test/test.dart';
import 'package:livekit_uniffi/livekit_uniffi.dart';

void main() {
  // Exercises real Rust JWT/HMAC logic across the FFI boundary: generate a
  // token, then verify and decode it.
  final creds = ApiCredentials(key: 'devkey', secret: 'devsecret');

  group('access token', () {
    test('generate produces a well-formed JWT', () {
      final token = tokenGenerate(
        options: TokenOptions(identity: 'alice', name: 'Alice'),
        credentials: creds,
      );
      // header.payload.signature
      expect(token.split('.'), hasLength(3));
    });

    test('verify round-trips identity, name, and issuer', () {
      final token = tokenGenerate(
        options: TokenOptions(identity: 'alice', name: 'Alice'),
        credentials: creds,
      );
      final claims = tokenVerify(token: token, credentials: creds);
      expect(claims.sub, equals('alice'));
      expect(claims.name, equals('Alice'));
      expect(claims.iss, equals('devkey'));
    });

    test('claims can be read without the secret', () {
      final token = tokenGenerate(
        options: TokenOptions(identity: 'bob'),
        credentials: creds,
      );
      final claims = tokenClaimsFromUnverified(token: token);
      expect(claims.sub, equals('bob'));
    });

    test('verify rejects a token signed with a different secret', () {
      final token = tokenGenerate(
        options: TokenOptions(identity: 'alice'),
        credentials: creds,
      );
      final wrong = ApiCredentials(key: 'devkey', secret: 'wrongsecret');
      expect(
        () => tokenVerify(token: token, credentials: wrong),
        throwsA(isA<AccessTokenException>()),
      );
    });
  });
}
