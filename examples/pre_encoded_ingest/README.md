# pre_encoded_ingest

End-to-end demo of the **pre-encoded video ingest** feature of the Rust
SDK. Pre-encoded H.264, H.265, VP8, or AV1 frames flow from a gstreamer
camera pipeline directly into `NativeEncodedVideoSource::capture_frame`,
get packetized by WebRTC (no software re-encode), and arrive at a
remote peer which writes decoded frames to a TCP port for a second
gstreamer pipeline to render.

```text
┌────────────┐ encoded (TCP) ┌─────────────┐   RTP (WebRTC)    ┌────────────┐   I420 (TCP)   ┌─────────────┐
│ gstreamer  │  ───────────►  │  sender.rs  │ ────────────────► │ receiver.rs│ ─────────────► │ gstreamer   │
│  (camera)  │   :5005       │ (pre-encoded│                   │  (decoded  │     :5006      │  (display)  │
│ tcpserver  │               │  publish,   │                   │   output)  │                │             │
│            │               │  tcp client)│                   │            │                │             │
└────────────┘               └─────────────┘                   └────────────┘                └─────────────┘
```

Gstreamer produces the encoded bytestream as a TCP server on :5005; the
Rust sender connects as a client and demuxes it into individual
frames. The sender supports two wire framings, picked by `--codec`:

- **H.264 / H.265** — raw Annex-B; the sender splits on AUD NAL
  boundaries.
- **VP8 / AV1** — IVF container (gstreamer's `ivfmux` or
  `avmux_ivf`); the sender parses the 32-byte file header (when
  present) and each 12-byte per-frame header. For AV1, each IVF
  record is one Temporal Unit (TU) — a complete OBU sequence for
  one frame.

## What this exercises

- `libwebrtc::video_source::NativeEncodedVideoSource` — the
  pre-encoded video track source, for `VideoCodec::H264`,
  `VideoCodec::H265`, `VideoCodec::Vp8`, and `VideoCodec::Av1`.
- Annex-B bytestream ingest (H.264/H.265), with automatic
  parameter-set caching and keyframe prepending done by the source
  (SPS/PPS for H.264, VPS/SPS/PPS for H.265) so the producer does not
  need to inline parameter sets on every IDR.
- IVF-framed ingest (VP8 / AV1) — no NAL parameter sets, one
  compressed frame per IVF record. Keyframe flag comes from bit 0 of
  the VP8 frame tag (RFC 6386) for VP8, or the presence of an
  `OBU_SEQUENCE_HEADER` (type 1) in the Temporal Unit for AV1 (AV1
  spec §5.3.2).
- `EncodedVideoSourceObserver` — keyframe-request and target-bitrate
  callbacks from the WebRTC pipeline.
- `LocalParticipant::publish_track` normalization for encoded sources
  (forces `simulcast=false` and remaps `video_codec` to match the
  source codec).

## Prerequisites

- gstreamer 1.22+ with the `good`, `bad`, `ugly`, and `libav` plugin
  sets:
  - macOS: `brew install gstreamer gst-plugins-base gst-plugins-good
    gst-plugins-bad gst-plugins-ugly gst-libav`
  - Debian/Ubuntu: `sudo apt install gstreamer1.0-tools
    gstreamer1.0-plugins-{base,good,bad,ugly} gstreamer1.0-libav`
- A LiveKit server (use `livekit-server --dev` locally or point at a
  cloud deployment).

# Validating Camera

**Before bringing LiveKit into the picture**, confirm your camera
encode path and a basic H.264 decode preview work in pure GStreamer.
The **send** and **receive** commands below use the **same UDP port
(5005)** on purpose: `udpsink` sends RTP to `127.0.0.1:5005` and `udpsrc`
binds `port=5005` for a quick local check.

That is only for this camera-validation hop. In the [full LiveKit
demo](#running-the-livekit-demo) below, **port 5005** is reserved for
**TCP** from the camera pipeline into `sender` (Annex-B bytestream),
and **port 5006** is where `receiver` serves **decoded I420** to a
separate GStreamer visualizer — different protocol, different payload,
and no overlap with this UDP/RTP smoke test.

### Send — camera → RTP/UDP 5005

macOS (`avfvideosrc`). Linux: replace the source with `v4l2src
device=/dev/video0`. Windows: `mfvideosrc device-index=0`. If the
camera cannot produce 640×480 natively, add `videoscale ! videorate !`
before `x264enc` and relax the first caps filter as needed.

```bash
gst-launch-1.0 -v \
  avfvideosrc ! \
  video/x-raw,width=640,height=480,framerate=30/1 ! \
  videoconvert ! \
  x264enc tune=zerolatency bitrate=1000 speed-preset=ultrafast key-int-max=30 ! \
  video/x-h264,profile=baseline ! \
  rtph264pay pt=96 config-interval=1 ! \
  udpsink host=127.0.0.1 port=5005
```

### Receive — RTP/UDP 5005 → display

```bash
gst-launch-1.0 -v \
  udpsrc port=5005 caps="application/x-rtp,media=video,encoding-name=H264,payload=96" ! \
  rtph264depay ! \
  avdec_h264 ! \
  videoconvert ! \
  autovideosink
```

On macOS, if `autovideosink` hangs at `PREROLLING` (common with
`glimagesink` under `gst-launch`), replace it with `osxvideosink`.

This path validates camera, encoder, and decoder. It is **not** the
same wire format as the Rust sender: the demo ingest uses **TCP** and
**Annex-B** with **AUD-delimited** access units (see the pipeline in
[Running the LiveKit demo](#running-the-livekit-demo)). For that path
you still want `x264enc … aud=true`, `h264parse`, and `tcpserversink` as
documented there.

### Debugging a blank / green receive window

Before blaming the network, collapse encode → decode into a single
local pipeline. A green square here means the encoder is being fed
buffers it cannot consume (wrong pixel format, GL memory, or no frames
at all):

```bash
gst-launch-1.0 -v \
    avfvideosrc device-index=0 ! \
    video/x-raw,width=640,height=480,format=NV12,framerate=30/1 ! \
    videoconvert ! \
    x264enc tune=zerolatency speed-preset=ultrafast bitrate=1000 key-int-max=60 aud=true ! \
    h264parse config-interval=1 ! avdec_h264 ! videoconvert ! autovideosink sync=false
```

Common causes of a green (or all-black) preview:

- **macOS camera permission.** Grant your terminal app Camera access
  in *System Settings → Privacy & Security → Camera* and relaunch it.
  Without permission, AVFoundation hands back solid green frames
  rather than failing.
- **`memory:GLMemory` on the source pad.** `avfvideosrc` often
  advertises GL-texture caps first; `x264enc` cannot consume them.
  Pinning `format=NV12` (or any other plain `video/x-raw` format) on
  the first caps filter forces a CPU buffer.
- **Caps pinned to a mode the camera cannot produce.** Run
  `gst-device-monitor-1.0 Video/Source` and pick a
  `width`/`height`/`format`/`framerate` combo listed under
  `video/x-raw` (not `video/x-raw(memory:GLMemory)`).

### Why TCP for the Rust ingest path (and not raw H.264 over UDP)?

The camera validation above uses **RTP** over UDP on localhost, where
packets stay small enough to avoid typical OS UDP limits.

For **raw Annex-B H.264** pushed with `udpsink`, macOS in particular has
a low default `net.inet.udp.maxdgram` (~9 KB), which large keyframes
can exceed. Symptoms look like:

```
Error sending message: Message too long
```

and broken or blocky video when the kernel drops datagrams. The demo
therefore uses **TCP** from GStreamer into `sender`, which has no such
per-write datagram cap.

## Running the LiveKit demo

### 0. Environment

```bash
export LIVEKIT_URL=ws://localhost:7880
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=secret
```

Both `sender` and `receiver` use `env_logger`, so they are silent
unless `RUST_LOG` is set. The step 2/3 invocations below already
prefix `RUST_LOG=info`; lower it to `warn` once the demo is running
clean, or raise it to `RUST_LOG=info,libwebrtc=debug` to see the
underlying C++ WebRTC log sink.

### 1. Start the gstreamer camera pipeline (Terminal 1)

**Annex-B over TCP** into the Rust sender (not the UDP/RTP validation
pipelines). `tcpserversink` listens on **TCP** port **5005**; stop any
other **TCP** listener on that port if you have one.

> **macOS — avoid TCP port 5000.** On macOS 12+ the *AirPlay Receiver*
> feature (managed by `ControlCenter`) binds `*:5000` by default.
> `tcpserversink host=0.0.0.0 port=5000` will log
> `Error binding to address 0.0.0.0:5000: Address already in use`,
> fall back to `current-port = 0`, and produce no data — while any
> client still "connects" to :5000 (it's talking to AirPlay, not to
> gstreamer). This demo uses **5005** to sidestep that. Either keep
> 5005, disable AirPlay Receiver in *System Settings → General →
> AirDrop & Handoff → AirPlay Receiver*, or pick another free port.
> Verify with `lsof -nP -iTCP:5005 -sTCP:LISTEN` — you should see
> `gst-launc`, not `ControlCe`.

macOS:

```bash
gst-launch-1.0 -v \
    avfvideosrc device-index=0 ! \
    video/x-raw,width=640,height=480,format=NV12,framerate=30/1 ! \
    videoconvert ! \
    x264enc tune=zerolatency speed-preset=ultrafast bitrate=1000 key-int-max=60 aud=true ! \
    h264parse config-interval=1 ! \
    video/x-h264,stream-format=byte-stream,alignment=au ! \
    tcpserversink host=0.0.0.0 port=5005
```

Linux: replace `avfvideosrc device-index=0` with `v4l2src device=/dev/video0`. Windows: `mfvideosrc device-index=0`.

Knobs that matter for `sender`:

- **`aud=true`** — NAL-type-9 AUD at the start of every access unit;
  the Rust sender splits the TCP byte stream on those boundaries.
- **`h264parse` … `stream-format=byte-stream,alignment=au`** — Annex-B
  suitable for the ingest path.
- **`tcpserversink`** accepts one TCP client at a time. Another
  process cannot listen on **TCP** :5005 at the same time. The RTP
  validation pipelines in [Validating Camera](#validating-camera) use
  **UDP** :5000, which is a different protocol *and* a different port,
  so the two setups do not interfere.

#### H.265 variant

For H.265/HEVC, swap the encoder and parser. `x265enc`'s AUD output is
controlled via `option-string`, which is forwarded to libx265:

```bash
gst-launch-1.0 -v \
    avfvideosrc device-index=0 ! \
    video/x-raw,width=640,height=480,format=NV12,framerate=30/1 ! \
    videoconvert ! \
    x265enc tune=zerolatency speed-preset=ultrafast bitrate=1000 key-int-max=60 \
        option-string="aud=1:repeat-headers=1" ! \
    h265parse config-interval=1 ! \
    video/x-h265,stream-format=byte-stream,alignment=au ! \
    tcpserversink host=0.0.0.0 port=5005
```

- `aud=1` emits the HEVC AUD (NAL type 35) at every AU boundary; the
  sender's splitter keys on those.
- `repeat-headers=1` makes libx265 inline VPS/SPS/PPS with every
  keyframe — cheap insurance in case the parser doesn't. The SDK
  source also caches and re-prepends parameter sets on its own, so
  either producer behaviour works.

You must pass `--codec h265` to `sender` as well (see step 2) so the
AU splitter uses the HEVC NAL-type layout. Mixing an H.265 pipeline
with a `--codec h264` sender will look like "no AUs ever flow" —
the 5-bit H.264 NAL-type mask won't find AUD=9 in an HEVC stream.

HEVC caveat: the **other peer** (receiver, SFU, JS client, etc.) must
actually be able to decode H.265. If the SDP answer strips the `H265`
payload type, nothing will be published even though `sender` logs look
healthy. Point-to-point between two instances of this demo on macOS
works because `RTCDefaultVideoDecoderFactory` exposes VideoToolbox
HEVC; your SFU's behaviour may differ.

#### VP8 variant

VP8 has no start codes, no NAL units, and no parameter sets, so we
need external framing. The sender consumes the **IVF** container
produced by gstreamer. Use `avmux_ivf` (from `gst-libav`) — it's the
most portable option and ships in Homebrew's consolidated `gstreamer`
formula:

```bash
gst-launch-1.0 -v \
    avfvideosrc device-index=0 ! \
    video/x-raw,width=640,height=480,format=NV12,framerate=30/1 ! \
    videoconvert ! \
    vp8enc deadline=1 cpu-used=5 threads=4 \
        target-bitrate=1000000 keyframe-max-dist=60 end-usage=cbr ! \
    avmux_ivf ! \
    tcpserversink host=0.0.0.0 port=5005
```

If your install has the native `ivfmux` element (gst-plugins-bad,
relatively recent versions), it's a drop-in replacement — the
Rust-side IVF parser only cares about the on-wire bytes, which are
identical. Check with `gst-inspect-1.0 ivfmux` / `gst-inspect-1.0
avmux_ivf`; `WARNING: erroneous pipeline: no element "ivfmux"` means
you have to use `avmux_ivf` (or reinstall gstreamer to pick up the
native muxer).

- The muxer emits a 32-byte file header once, followed by a 12-byte
  per-frame header + payload. The sender parses exactly that shape.
- `target-bitrate` is in **bps** (unlike `x264enc`/`x265enc` which use
  kbps). The example above is 1 Mbps.
- `keyframe-max-dist=60` matches the 60-frame IDR interval used by the
  H.26x pipelines, so time-to-first-frame behaves the same.
- `deadline=1` is realtime mode; `cpu-used=5` is the fastest preset.

Keep `--codec vp8` on the sender (step 2). VP8 is the baseline
WebRTC codec, so SFU/peer compatibility is not a concern.

> The `DKIF` file header is optional on the wire. The native
> `ivfmux` element emits it; `avmux_ivf` (libav-backed) swallows it
> on a non-seekable sink like `tcpserversink` and emits only
> per-frame records. The sender handles both: it consumes `DKIF` if
> the first four bytes match, otherwise it starts parsing 12-byte
> per-frame records directly. Gstreamer's one-buffer-per-packet
> semantics keep every `tcpserversink` client frame-aligned, so
> start-order between sender and gstreamer does not matter for VP8.
> If the reader ever parses an absurd `frame_size`, it drops the
> TCP connection and reconnects to re-align on the next buffer.

#### AV1 variant

AV1 rides the same IVF wire format as VP8 (FOURCC `AV01`). The
sender treats each IVF record as a complete Temporal Unit (TU) — the
OBU sequence for one frame — and detects keyframes by scanning the
TU's OBUs for an `OBU_SEQUENCE_HEADER` (type 1), which libaom,
SVT-AV1, and rav1e only emit at keyframes.

Use `av1enc` (libaom, in `gst-plugins-bad`). You also want `av1parse`
between the encoder and the muxer so OBUs land in the Low Overhead
Bitstream Format with size fields populated and one TU per buffer:

```bash
gst-launch-1.0 -v \
    avfvideosrc device-index=0 ! \
    video/x-raw,width=640,height=480,format=NV12,framerate=30/1 ! \
    videoconvert ! \
    av1enc usage-profile=realtime end-usage=cbr cpu-used=9 \
        target-bitrate=1000 keyframe-max-dist=60 threads=4 ! \
    av1parse ! \
    video/x-av1,stream-format=obu-stream,alignment=tu ! \
    avmux_ivf ! \
    tcpserversink host=0.0.0.0 port=5005
```

Pass `--codec av1` to the sender (step 2). Notes on the AV1 encoder:

- **`av1enc target-bitrate` is in kbps** (libaom convention), unlike
  `vp8enc` which uses bps. The example above is 1 Mbps.
- `usage-profile=realtime` + `end-usage=cbr` picks libaom's realtime
  rate-control path; without it the default is high-latency good-
  quality mode and frames arrive in bursts.
- `cpu-used` for libaom AV1 realtime is 0..=10 (higher = faster,
  lower quality). 9 is a reasonable live-capture default on a
  laptop-class CPU; drop to 7 if your CPU is idle and you want
  better quality at the same bitrate. If `ingest: X fps accepted`
  lags your capture framerate, bump `cpu-used` or raise `threads`
  (libaom AV1 is CPU-hungry).
- `keyframe-max-dist=60` mirrors the other pipelines for identical
  time-to-first-frame.
- `av1parse` normalises the bitstream to OBU-stream framing aligned
  on Temporal Units, which is what `avmux_ivf` expects and what the
  Rust sender's keyframe probe assumes. Leaving it out usually still
  works but is encoder-dependent — keep it in the pipeline.

Alternative encoders (same pipeline shape, only the encoder element
changes):

- **SVT-AV1** (`svtav1enc`, `gst-plugins-bad`) — faster than libaom
  at comparable quality; tuning knobs differ
  (`preset=10 target-bitrate=1000 rate-control-mode=cbr`).
- **rav1e** (`rav1enc`, `gst-plugins-rs`) — pure-Rust AV1 encoder;
  realtime-ish at low `speed-preset` values.

Keep `--codec av1` on the sender regardless of which AV1 encoder you
pick — the Rust side only cares about the on-wire IVF/OBU bytes.

> **AV1 peer compatibility.** Like H.265, the receiving peer must
> actually be able to decode AV1. All recent browsers ship a dav1d
> decoder and LiveKit's default C++ factory also enables dav1d via
> `RTC_DAV1D_IN_INTERNAL_DECODER_FACTORY`, so macOS-to-macOS (two
> instances of this demo) and browser subscribers work out of the
> box. Older SFUs may strip the AV1 payload type from the SDP
> answer; `sender` will log happy ingest stats while the peer sees
> a black frame.

The IVF-header-optional notes apply here too: native `ivfmux` emits a
`DKIF` header with FOURCC `AV01`; `avmux_ivf` on `tcpserversink`
does not. The sender handles both.

### 2. Start the sender (Terminal 2)

```bash
RUST_LOG=info cargo run -p pre_encoded_ingest --bin sender -- \
    --tcp-host 127.0.0.1 --tcp-port 5005 \
    --width 640 --height 480 \
    --codec h264 \
    --room pre-encoded-demo --identity encoded-sender
```

For the H.265 pipeline use `--codec h265`; for VP8 use `--codec vp8`;
for AV1 use `--codec av1`.

Flags:

- `--tcp-host/--tcp-port` where gstreamer's `tcpserversink` is
  listening.
- `--width/--height` declared stream resolution; must match what
  gstreamer is producing.
- `--codec {h264,h265,vp8,av1}` selects the wire framing and keyframe
  probe: Annex-B (AUD-split) for H.264/H.265, or IVF for VP8/AV1.
  **Must match the gstreamer pipeline.** `publish_track` will
  additionally remap the track's `video_codec` to match the source,
  so the LiveKit publish options follow automatically.

The sender logs one line every ~2 s with ingest stats and will print
warnings when the receiver requests keyframes or when the congestion
controller updates the target bitrate. If the gstreamer pipeline is
restarted, the sender reconnects automatically.

### 3. Start the receiver (Terminal 3)

```bash
RUST_LOG=info cargo run -p pre_encoded_ingest --bin receiver -- \
    --tcp-port 5006 \
    --room pre-encoded-demo --identity encoded-receiver \
    --from encoded-sender
```

The receiver subscribes to the room and waits for a TCP client on the
given port. Each decoded I420 frame is written tightly packed
(Y ‖ U ‖ V, no row padding, no framing header) on the socket.

### 4. Visualize (Terminal 4)

```bash
gst-launch-1.0 -v \
    tcpclientsrc host=127.0.0.1 port=5006 ! \
    rawvideoparse width=640 height=480 format=i420 framerate=30/1 ! \
    videoconvert ! autovideosink sync=false
```

`rawvideoparse` needs the exact width/height the receiver is producing.
If the publisher is at 640x480, use `width=640 height=480` here.
Framerate just drives gstreamer's display pacing — the Rust side
writes frames as fast as WebRTC delivers them.

> The receiver's TCP output is **raw I420**, not H.264. Do **not**
> pipe it through `h264parse` — you will see
> `h264parse: No valid frames found before end of stream` /
> `Broken bit stream` because the bytes are Y/U/V planes, not NAL
> units. Use `rawvideoparse` as shown above.

If the publisher resolution changes mid-run, the receiver closes the
TCP connection; reconnect your gstreamer visualizer to pick up the
new caps.

## Troubleshooting

**Sender connects to the room but never logs ingest stats.**
Most often the Rust sender is connected to something that is not
gstreamer. Quick checks, in order:

1. Confirm the gstreamer pipeline from step 1 is actually running and
   logging `PLAYING`, not blocked on `Address already in use`.
2. Sniff the TCP stream directly — you should see NAL-unit bytes
   flowing:

   ```bash
   nc 127.0.0.1 5005 | pv -b > /dev/null
   ```

   If `pv` stays at `0 B`, the other end is not gstreamer (on macOS,
   most commonly AirPlay Receiver on :5000; see the macOS callout in
   step 1).
3. Confirm you picked the TCP Annex-B pipeline from step 1 and not the
   UDP/RTP validation pipeline from [Validating Camera](#validating-camera) —
   the latter won't feed `tcpserversink`.

**gstreamer says `WARNING: erroneous pipeline: no element "ivfmux"`.**
Your gstreamer install doesn't bundle the native IVF muxer. Swap
`ivfmux` for `avmux_ivf` (from `gst-libav`), which produces an
identical IVF byte stream and is in Homebrew's consolidated
`gstreamer` formula. Confirm with `gst-inspect-1.0 avmux_ivf`. If
neither is present, `brew reinstall gstreamer` (or on Debian/Ubuntu,
`sudo apt install gstreamer1.0-libav gstreamer1.0-plugins-bad`) will
pull both in.

**gstreamer reports `Error binding to address 0.0.0.0:5000`.**
Another process is listening on that port. On macOS that is usually
AirPlay Receiver; use port 5005 (as this README does) or disable
AirPlay Receiver. Check with:

```bash
lsof -nP -iTCP:5000 -sTCP:LISTEN
```

**Visualizer shows `h264parse: No valid frames found` / `Broken bit
stream` / `No caps set`.**
The visualizer in step 4 is consuming the receiver's output
(port 5006), which is raw I420 — not H.264. Use `rawvideoparse` as
shown, not `h264parse`. `h264parse` belongs in step 1, on the
*sender* side.

**Nothing logs at all from the Rust binaries.**
`sender`/`receiver` use `env_logger`; set `RUST_LOG=info` (as in the
commands above). Without it, both processes are silent even when they
are working correctly.

**Sender connects to gstreamer, TCP bytes flow, but `ingest:` still
reads 0 fps accepted.**
Almost always a codec / framing mismatch between the gstreamer
pipeline and the sender:

- **H.26x**: the demuxer looks for the AUD NAL type of whichever codec
  you passed via `--codec` (9 for H.264, 35 for H.265), and the two
  use different bit layouts for the NAL-type field. An H.265 stream
  fed to `--codec h264` (or vice versa) will scan end-to-end without
  ever recognising an AUD boundary, so no AU is ever pushed to
  `capture_frame`.
- **VP8 / AV1**: the demuxer accepts IVF with or without the `DKIF`
  file header (native `ivfmux` emits it; `avmux_ivf` on
  `tcpserversink` doesn't). It assumes the first byte starts an IVF
  per-frame record, which is what gstreamer's one-buffer-per-packet
  delivery guarantees. If you see `IVF: implausible frame_size=N
  bytes`, gstreamer produced a byte stream where the first byte of a
  new client's delivery is mid-packet (very rare in practice). The
  sender logs the warning, drops the TCP connection, and reconnects —
  which usually re-anchors on the next buffer boundary. If it keeps
  happening, your muxer is producing non-record-aligned buffers; swap
  `avmux_ivf` for the native `ivfmux` if it's available. If you pass
  a `--codec` that doesn't match the pipeline's FOURCC (e.g.
  `--codec av1` on a VP8 stream), you'll get a one-shot warning from
  the IVF reader but bytes will keep flowing — the FOURCC check is
  advisory; what actually differs between the IVF-framed codecs is
  the keyframe probe (RFC 6386 frame-tag bit for VP8, OBU sequence-
  header scan for AV1).
- **AV1-specific**: if ingest accepts frames but the receiver never
  decodes them, check that your pipeline includes
  `av1parse ! video/x-av1,stream-format=obu-stream,alignment=tu`
  before `avmux_ivf`. Some encoders emit OBUs without size fields
  when fed directly to the muxer; the sender's keyframe probe can't
  skip those reliably and will mark every frame as a delta, causing
  the jitter buffer to wait forever for a keyframe.
- **Mixed**: `--codec vp8` pointed at an Annex-B H.264 pipeline (or
  `--codec h264` at an IVF VP8 pipeline) will either trip the IVF
  magic check or silently scan forever — re-check `--codec` matches
  your pipeline.

**H.265 track publishes, but the remote peer shows a black frame.**
The other peer cannot decode HEVC — check the SDP answer for an
`H265` payload type. LiveKit SFUs that support H.265 will forward;
ones that don't will either drop the subscription or fall through to
a fallback codec. Point two instances of this demo at the same room
on macOS to isolate whether the problem is the SDK or the SFU:
VideoToolbox HEVC is available in `RTCDefaultVideoDecoderFactory`, so
macOS-to-macOS should decode cleanly.

## Known limitations

### VP9 is not documented as a supported codec for this example

`CodecArg::Vp9` still exists in `sender.rs` (and
`NativeEncodedVideoSource` accepts `VideoCodec::Vp9`), but VP9 ingest
is not exercised by this demo and has rough edges that make it a poor
fit for a "pre-encoded bytes straight to RTP" path:

- libvpx-vp9 emits **superframes** in IVF (a per-frame record can
  bundle several coded frames — e.g. a show_existing_frame reshow
  plus a hidden alt-ref). WebRTC's VP9 RTP packetizer expects one
  *coded* frame per input, so feeding a superframe as one
  `capture_frame` call misreports keyframe-ness and confuses the
  depacketizer on the peer.
- Keyframe detection from just the VP9 uncompressed header misses
  show_existing_frame / alt-ref semantics that determine whether a
  picture actually refreshes the reference buffers.
- SVC (spatial / temporal layering) — the main reason to pick VP9
  over VP8 — needs the VP9 RTP descriptor plumbed through the encoded
  source, which this branch does not expose.

For single-layer VP9 with patched-up superframe handling this could
be revisited, but today **use VP8 or AV1** for IVF-framed ingest.
`--codec vp9` is left in the CLI so existing scripts don't break; it
is intentionally undocumented here.

### Receive-side encoded frames are not exposed

The feature added in this branch covers the **send** side: the producer
hands encoded bytes in, WebRTC packetizes them out. On the **receive**
side the SDK currently only exposes decoded frames via
`NativeVideoStream`. That's why the receiver round-trips through
WebRTC's internal decoder and serves raw I420 to gstreamer, rather
than forwarding encoded H.264.

Exposing encoded frames on receive would require a
`RemoteEncodedVideoStream` analogue (likely backed by a WebRTC
`FrameTransformer`) and is a natural follow-up.

### AUD-delimited bytestreams only

The sender relies on `x264enc aud=true` emitting a NAL-type-9 AUD at
the start of every AU so it can find frame boundaries over the TCP
byte stream. Producers that don't emit AUDs would need a richer
splitter (e.g. detecting "new primary coded picture" via the slice
header's `first_mb_in_slice`).

### Keyframe intervals dominate startup latency

WebRTC's jitter buffer drops delta frames until it sees a keyframe, so
time-to-first-frame on the receiver is bounded by the x264enc
`key-int-max`. Lower `key-int-max` for faster startup at the cost of
bitrate overhead.
