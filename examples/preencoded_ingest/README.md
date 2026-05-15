# preencoded_ingest

Publish a pre-encoded H.264 video stream into a LiveKit room **without re-encoding it**.

This example demonstrates the new pre-encoded video pipeline (`NativeEncodedVideoSource` + the C++ `PassthroughVideoEncoder`). Bytes flow straight from your source onto the wire, which is useful when you already have an encoded bitstream from a hardware capture device, a transcoder, an upstream RTSP/SRT feed, or a gstreamer pipeline.

## How it works

1. `TcpH264Source` connects out to a TCP server (we are the *client*; gstreamer's `tcpserversink` is the listener) and reads raw bytes.
2. `AnnexBParser` splits the byte stream into NALUs.
3. `FrameAssembler` groups NALUs into complete H.264 access units, prepending cached SPS/PPS to keyframes that arrive without inline parameter sets, and dropping anything that arrives before the first decodable IDR.
4. Each assembled frame is handed to `NativeEncodedVideoSource::capture_frame(...)`.
5. The C++ side queues the bytes, kicks the WebRTC encode pipeline with a 2x2 dummy frame, and the paired `PassthroughVideoEncoder` dequeues + emits the original payload via `OnEncodedImage()`.

The TCP + parser plumbing is hidden behind a small `EncodedFrameSource` trait so you can swap `TcpH264Source` for a different transport (file, named pipe, gRPC, ...) without touching `main.rs`.

## Prerequisites

- A LiveKit server (URL + API key/secret).
- A producer that emits Annex-B H.264 over TCP. The example commands below use `gst-launch-1.0` for convenience but anything that speaks the same wire format works.

## LiveKit credentials

Pass them via flags or environment variables:

- `--url` or `LIVEKIT_URL`
- `--api-key` or `LIVEKIT_API_KEY`
- `--api-secret` or `LIVEKIT_API_SECRET`

## Running it

Start a gstreamer pipeline that emits 1280x720 H.264 over TCP:

```bash
gst-launch-1.0 -v videotestsrc is-live=true \
  ! video/x-raw,width=1280,height=720,framerate=30/1 \
  ! x264enc tune=zerolatency key-int-max=30 bitrate=2000 \
  ! video/x-h264,stream-format=byte-stream,alignment=au \
  ! tcpserversink host=0.0.0.0 port=5000
```

Then publish into a LiveKit room:

```bash
cargo run -p preencoded_ingest -- \
  --room demo \
  --connect 127.0.0.1:5000
```

Or with explicit credentials:

```bash
cargo run -p preencoded_ingest -- \
  --url wss://your.livekit.server \
  --api-key YOUR_KEY \
  --api-secret YOUR_SECRET \
  --room demo \
  --connect 127.0.0.1:5000 \
  --width 1280 --height 720 \
  --identity preencoded-publisher \
  --track-name preencoded
```

## Flags

- `--connect <host:port>`: TCP server to read the H.264 stream from. Default `127.0.0.1:5000`.
- `--width <px>`, `--height <px>`: Declared video dimensions. Must match the encoded stream's resolution. Defaults `1280x720`.
- `--room <name>`: Room to join. Default `preencoded-demo`.
- `--identity <id>`: Identity to publish under. Default `preencoded-publisher`.
- `--track-name <name>`: Track name surfaced to subscribers. Default `preencoded`.

Common LiveKit flags (`--url` / `--api-key` / `--api-secret`) accept the corresponding `LIVEKIT_*` env vars as fallbacks.

## Frame metadata defaults

`source.capture_frame(&info)` auto-fills `FrameMetadata` with:

- `user_timestamp`: current `SystemTime` (microseconds since UNIX epoch)
- `frame_id`: per-source monotonically increasing counter
- `has_packet_trailer: true`

If a [`PacketTrailerHandler`](../../libwebrtc/src/native/packet_trailer.rs) is later attached to the source via `set_packet_trailer_handler(...)`, the metadata is automatically embedded in the egress RTP packets so receivers can extract them.

For caller-controlled metadata (custom `user_timestamp` / `frame_id`, or to opt out of the trailer entirely), use `source.capture_frame_with_metadata(&info, &metadata)` instead.

## Swapping the ingest source

Implement [`EncodedFrameSource`](src/source.rs) for your transport and replace the `TcpH264Source::connect(...)` call in [`main.rs`](src/main.rs):

```rust
let mut frames: Box<dyn EncodedFrameSource> = Box::new(MyFileSource::open(path)?);
```

The trait is intentionally narrow:

```rust
pub trait EncodedFrameSource: Send {
    fn next_frame<'a>(
        &'a mut self,
    ) -> Pin<Box<dyn Future<Output = Result<Option<EncodedFrame>>> + Send + 'a>>;
}
```

`Ok(None)` signals end-of-stream; `Err(_)` aborts the publish loop.

## Caveats

- Only Annex-B H.264 is parsed in this example. The SDK supports H.265 / VP8 / VP9 / AV1 via the `VideoCodecType` enum on `NativeEncodedVideoSource::new(...)`, but you would need a parser+assembler appropriate for that codec.
- Simulcast is automatically disabled when publishing an encoded source (the passthrough encoder produces a single layer).
- `--width`/`--height` must match the actual encoded stream; mismatches will confuse downstream decoders.
- The example publishes with `TrackSource::Camera` for convenience -- adjust `TrackPublishOptions` in `main.rs` if you want a different `TrackSource`.
