# LiveKit Rust SDKs threat model

`livekit/rust-sdks` is the Rust client SDK for LiveKit, plus the native layer
that other language SDKs build on. A client joins a room over a WebSocket
signalling connection and exchanges media and data with other participants
over WebRTC. `livekit-ffi` exposes the client to host languages in two ways: a
C ABI that takes protobuf requests (`room-apis`, used by the Python, Node, and
Unity SDKs) and UniFFI bindings (`core-modules`: data streams, data tracks,
signalling, and tokens, used from Swift and Kotlin).
`livekit-api` and `livekit-token` are server-side helpers for minting tokens,
verifying webhooks, and calling the server API.

## Scope and trust boundaries

Audit the hand-written Rust and C++ on `main`. Prioritise:

- `livekit/src/rtc_engine/` and `livekit-signaling/`: signalling from the
  server (`rtc_session.rs`, `signal_stream.rs`) and handling of data packets
  received from other participants.
- `livekit-data-stream/src/incoming/`, `livekit-rpc/src/server.rs`, and
  `livekit-datatrack/src/remote/` and `src/packet/`: reassembly and parsing of
  data streams, RPC requests, and data-track packets sent by other
  participants.
- `livekit/src/room/e2ee/` and `livekit-datatrack/src/e2ee.rs`: key provider,
  ratcheting, and encryption of media frames, data packets, and data tracks.
- `webrtc-sys/src/*.cpp`: LiveKit's C++ bridge to libwebrtc, especially
  `packet_trailer*.cpp`, `av1_bitstream.cpp`, `frame_cryptor.cpp`, and the
  video and audio frame-buffer bridges.
- Unsafe Rust over frame buffers: `libwebrtc/src/native/video_frame.rs`,
  `libwebrtc/src/native/yuv_helper.rs`, `imgproc/src/colorcvt/`,
  `livekit/src/plugin.rs`, `soxr-sys/src/soxr.rs`,
  `livekit-ffi/src/room_apis/server/colorcvt/`, and
  `livekit-ffi/src/room_apis/server/resampler.rs`. Frame dimensions, strides, and
  sample counts from remote tracks reach these.
- `yuv-sys` and `soxr-sys`: build scripts and bindings for the vendored libyuv
  and libsoxr.
- `livekit-token/src/access_token.rs` (`TokenVerifier::verify`,
  `Claims::from_unverified`, grants) and `livekit-api/src/webhooks.rs`
  (`WebhookReceiver::receive`).
- `livekit-ffi/src/room_apis/server/` and `src/core_modules/`: conversion
  of remote events into FFI events.

Out of scope:

- Generated code: `livekit-protocol/src/livekit*.rs`, `logger*.rs`,
  `livekit-api/src/public_api/generated.rs`, and prost/uniffi output.
- The `examples/`, `tools/`, and `livekit-ffi-node-bindings/` directories.
- The NVIDIA, Jetson, and VAAPI hardware codecs. This image does not build the
  NVIDIA or Jetson paths.

Untrusted input:

- Everything from other participants: data packets, data streams, RPC
  requests and responses, data-track packets, participant metadata and
  attributes, media frames and their packet trailers, and encrypted payloads.
- Signalling from the server. For E2EE, the server is untrusted: it must not be
  able to read or forge encrypted content.
- Webhook requests and access tokens given to `livekit-api` and
  `livekit-token` verifiers.

Trusted:

- The host application on the far side of the FFI boundary, through either
  the C ABI or the UniFFI bindings. FFI requests can carry raw pointers
  (`data_ptr` fields), so a host that passes a bad pointer or handle is not a
  finding.
- API secrets, E2EE keys, and the options the application supplies.

LiveKit server (`livekit/livekit`) is enrolled separately. Report
server-side bugs there.

## Security properties

- **Memory safety:** no data from a remote participant or the server can cause
  out-of-bounds access, use-after-free, or other undefined behaviour in unsafe
  Rust, in `webrtc-sys` C++, or in the libyuv and libsoxr bindings.
- **No remote crash:** malformed signalling, data packets, data streams, RPC
  payloads, or data-track packets must not panic, loop, or allocate without
  bound. Release FFI builds use `panic = "abort"`, so a panic there terminates
  the host application.
- **E2EE:** without the key, the server and other participants cannot decrypt
  frames, data packets, or data tracks, or forge content that decrypts. Key
  derivation and ratcheting follow the configured algorithm, and decryption
  failures must not fall back to plaintext.
- **Participant isolation:** a participant must not be able to spoof another
  participant's identity on data, stream, RPC, or data-track messages, or
  interfere with streams or RPC calls it does not own.
- **Token verification:** `TokenVerifier::verify` accepts only HS256 tokens
  signed with the configured secret, issued by the configured key, and inside
  their validity window. `Claims::from_unverified` must never be treated as
  verification.
- **Webhook verification:** an event is accepted only when its token verifies
  and the body's SHA-256 matches the signed hash.
- **Secrets:** API secrets and E2EE keys must not appear in logs, errors, or
  FFI events.

## Severity guidance

- **Critical:** memory corruption that a remote participant or the server can
  trigger and that leads to code execution. Breaking E2EE confidentiality or
  integrity. Forging a token or webhook without the secret.
- **High:** remotely triggerable out-of-bounds read or memory corruption
  without demonstrated code execution. A panic or abort that any remote
  participant can trigger. Spoofing another participant's identity. Accepting
  an expired, wrong-algorithm, or wrong-issuer token.
- **Medium:** unbounded memory or CPU use from one remote input. Key or secret
  leakage into logs. A panic that only the server can trigger without E2EE in
  use.
- **Low / not a finding:** misuse that requires a malicious host application
  or bad FFI pointers. Issues only in generated code, examples, or tools.

## Testing

The checkout is at `/src`, with submodules. Cargo dependencies are fetched and
`CARGO_NET_OFFLINE=true` is set. libwebrtc is prebuilt at
`/opt/libwebrtc/current` (`LK_CUSTOM_WEBRTC`), and clang 21 is at
`/opt/llvm/bin`. No LiveKit server is available, so tests behind
`__lk-e2e-test` cannot run.

These commands match what the image prebuilt. Other packages, features, or
`--release` work offline but recompile, which is slow on this machine.

```sh
cd /src
cargo test \
  -p livekit -p livekit-api -p livekit-protocol -p livekit-token -p livekit-token-source \
  -p livekit-signaling -p livekit-net -p livekit-region -p livekit-common -p livekit-telemetry \
  -p livekit-data-stream -p livekit-datatrack -p livekit-rpc -p livekit-ffi \
  -p libwebrtc -p webrtc-sys -p webrtc-sys-build -p yuv-sys -p soxr-sys -p imgproc -p device-info
cargo test -p livekit-ffi --no-default-features --features core-modules
```

## Reports

State the attacker's required position (another participant in the room, the
signalling server, or a webhook sender), the entry point, and the demonstrated
impact. Provide a minimal reproducer, preferably a Rust test in the affected
crate.
