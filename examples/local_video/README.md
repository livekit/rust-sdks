# local_video

Examples demonstrating capturing frames from a local camera video and publishing to LiveKit, listing camera capabilities, subscribing to render video in a window, and showing a low-latency clock for measurement.

**Note:** These examples are intended for **desktop platforms only** (macOS, Linux, Windows).
You must enable the `desktop` feature when building or running them.
For smoother local rendering, especially above 720p, run the publisher/subscriber with `cargo run --release`.

- list_devices: enumerate available cameras and their capabilities
- publisher: capture from a selected camera and publish a video track
- subscriber: connect to a room, subscribe to video tracks, and display in a window
- clock: render a high-contrast wall-clock with three millisecond digits and a millisecond grid

LiveKit connection can be provided via flags or environment variables:
- `--url` or `LIVEKIT_URL`
- `--api-key` or `LIVEKIT_API_KEY`
- `--api-secret` or `LIVEKIT_API_SECRET`

Publisher usage:
```
 cargo run -p local_video -F desktop --bin publisher -- --list-cameras
 cargo run -p local_video -F desktop --bin publisher -- --camera-index 0 --room-name demo --identity cam-1
 
 # with explicit LiveKit connection flags
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --simulcast \
   --codec h265 \
   --max-bitrate 1500000 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET

 # publish with a user timestamp attached to every frame
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --attach-timestamp

 # publish with timestamp burned into the video and a frame ID in the packet trailer
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --attach-timestamp \
   --burn-timestamp \
   --attach-frame-id

 # publish at a custom resolution and framerate
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --width 1920 \
   --height 1080 \
   --fps 60 \
   --room-name demo \
   --identity cam-1

 # request MJPEG camera capture to reduce USB bandwidth
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --format mjpeg \
   --room-name demo \
   --identity cam-1

 # publish from a Jetson MIPI CSI camera through libargus and the Jetson hardware encoder
 cargo run -p local_video -F desktop --bin publisher -- \
   --source argus \
   --camera-index 0 \
   --codec h265 \
   --room-name demo \
   --identity jetson-cam-1

 # publish AV1 through the Jetson hardware encoder (Orin only)
 cargo run -p local_video -F desktop --bin publisher -- \
   --source argus \
   --camera-index 0 \
   --codec av1 \
   --room-name demo \
   --identity jetson-cam-1

 # publish a static SMPTE color-bar test pattern (no camera required)
 cargo run -p local_video -F desktop --bin publisher -- \
   --test-pattern \
   --room-name demo \
   --identity test-1

 # publish a low-latency test pattern with timestamp metadata
 cargo run -p local_video -F desktop --bin publisher -- \
   --test-pattern \
   --width 1280 \
   --height 720 \
   --fps 30 \
   --codec h264 \
   --degradation-preference maintain-resolution \
   --min-playout-delay 0 \
   --max-playout-delay 1 \
   --attach-timestamp \
   --attach-frame-id \
   --room-name demo \
   --identity test-1

 # publish with end-to-end encryption
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --e2ee-key my-secret-key

 # publish and display the outgoing video locally
 cargo run -p local_video -F desktop --bin publisher -- \
   --camera-index 0 \
   --room-name demo \
   --identity cam-1 \
   --display-video
```

List devices usage:
```
 cargo run -p local_video -F desktop --bin list_devices
```

Clock usage:
```
 cargo run -p local_video -F desktop --bin clock
 cargo run --release -p local_video -F desktop --bin clock -- --fullscreen
```

Clock flags:
- `--fullscreen`: Start in borderless fullscreen.
- `--always-on-top`: Keep the clock above normal windows.
- `--no-vsync`: Disable vsync and render as fast as the display backend accepts frames. By default the clock uses WGPU with vsync and a maximum frame latency of 1 to avoid uncapped GPU usage.

The clock draws a 3x9 grid below the time. The top row fills from `0` to `9` for the hundreds-of-milliseconds digit, the middle row for tens of milliseconds, and the bottom row for ones of milliseconds.

Publisher flags (in addition to the common connection flags above):
- `--camera-index <n>`: Camera index to use (default: `0`). Use `--list-cameras` to see available indices.
- `--source <uvc|argus>`: Camera backend to use (default: `uvc`). `argus` uses NVIDIA libargus for MIPI CSI cameras and is available only on Linux aarch64 Jetson builds.
- `--format <auto|yuv|mjpeg>`: UVC camera capture format (default: `auto`). `auto` tries uncompressed YUYV first and falls back to MJPEG; `mjpeg` can reduce USB bandwidth when running multiple cameras.
- `--test-pattern`: Generate a standard SMPTE 75% color-bar test pattern instead of capturing from a camera. `--source`, `--camera-index`, and `--format` are ignored when this is set; `--width`, `--height`, and `--fps` still control the output resolution and frame rate.
- `--width <px>`: Desired capture width (default: `1280`).
- `--height <px>`: Desired capture height (default: `720`).
- `--fps <n>`: Desired capture framerate (default: `30`).
- `--codec <codec>`: Video codec to use for publishing: `h264`, `h265`, `vp8`, `vp9`, or `av1` (default: `h264`). H.265 falls back to H.264 on failure. On Jetson Orin, `h264`, `h265`, and `av1` use the hardware encoder; elsewhere `av1` is encoded in software via libaom.
- `--simulcast`: Publish simulcast video (multiple layers when the resolution is large enough).
- `--max-bitrate <bps>`: Max video bitrate for the main (highest) layer in bits per second (e.g. `1500000`).
- `--degradation-preference <disabled|maintain-framerate|maintain-resolution|balanced>`: Set WebRTC sender adaptation behavior. Use `maintain-resolution` for fixed-resolution latency benchmarks where frame-rate or quality drops are preferable to resolution ramping/downscaling. The publisher logs a warning and continues if libwebrtc rejects this sender hint.
- `--min-playout-delay <ms>`: Publisher-side room setting for the subscriber playout-delay minimum.
- `--max-playout-delay <ms>`: Publisher-side room setting for the subscriber playout-delay maximum. `0` disables the room max; use `1` for the smallest active max delay in low-latency tests.
- `--attach-timestamp`: Attach the current wall-clock time (microseconds since UNIX epoch) as the user timestamp on each published frame. The subscriber can display this to measure end-to-end latency. The publisher also logs `Publisher frame latency` windows that break capture-to-packetize into buffer, encoder, and packetize stages.
- `--burn-timestamp`: Burn the attached timestamp into the video frame as a visible overlay. Has no effect unless `--attach-timestamp` is also set.
- `--attach-frame-id`: Attach a monotonically increasing frame ID to each published frame via the packet trailer. The subscriber displays this in the timestamp overlay when `--display-timestamp` is used.
- `--display-video`: Open a window that displays the video frames being published.
- `--display-timing`: Burn publisher timing metrics into the local preview window. Requires `--display-video`.
- `--e2ee-key <key>`: Enable end-to-end encryption with the given shared key. The subscriber must use the same key to decrypt.

Subscriber usage:
```
 # relies on env vars LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET
 cargo run -p local_video -F desktop --bin subscriber -- --room-name demo --identity viewer-1

 # or pass credentials via flags
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --url https://your.livekit.server \
   --api-key YOUR_KEY \
   --api-secret YOUR_SECRET

 # subscribe to a specific participant's video only
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --participant alice

 # display timestamp overlay (requires publisher to use --attach-timestamp)
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --display-timestamp

 # minimize subscriber-side UI/stats overhead while preserving render timing logs
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --no-overlay \
   --no-stats

 # subscribe with end-to-end encryption (must match publisher's key)
 cargo run -p local_video -F desktop --bin subscriber -- \
   --room-name demo \
   --identity viewer-1 \
   --e2ee-key my-secret-key
```

Subscriber flags (in addition to the common connection flags above):
- `--participant <identity>`: Only subscribe to video tracks from the specified participant.
- `--display-timestamp`: Show a top-left overlay with frame ID, the publisher's timestamp, the subscriber's current time, and the computed end-to-end latency. Timestamp fields require the publisher to use `--attach-timestamp`; frame ID requires `--attach-frame-id`.
- `--no-overlay`: Hide the subscriber HUD and controls while still logging render timing from the paint callback. This is useful for latency soaks where UI drawing should not affect the measurement.
- `--no-stats`: Disable periodic WebRTC `getStats()` polling. Render timing logs still run, but codec, bitrate, decoder, jitter-buffer, and decode-health stats are not refreshed.
- `--headless`: Consume the subscribed video stream without opening a render window. This logs sink delivery and optional WebRTC stats but does not produce render latency windows.
- `--render-vsync`: Use vsync presentation for the subscriber render window. This can reduce GPU/display contention when checking visible render smoothness.
- `--keep-window-front`: Focus the subscriber render window and keep it above other windows. This is useful for visible latency benchmarks on desktop OSes where occluded or background windows may be throttled.
- `--render-loop-diagnostics`: Log subscriber render-loop scheduling windows. Use this for diagnostic runs when visible render latency has stutters and you need to separate app update gaps, WGPU prepare/import duration, and paint cadence.
- `--drop-late-frames-ms <ms>`: Drop decoded frames older than this before handing them to the render loop. The default `0` disables late-frame dropping; use this only when you want visible latency bounded after subscriber stalls and will track the drop count in reports.
- `--e2ee-key <key>`: Enable end-to-end decryption with the given shared key. Must match the key used by the publisher.

Notes:
- If the active video track is unsubscribed or unpublished, the app clears its state and will automatically attach to the next matching video track when it appears.
- For E2EE to work, both publisher and subscriber must specify the same `--e2ee-key` value. If the keys don't match, the subscriber will not be able to decode the video.
- The timestamp overlay updates at ~2 Hz so the latency value is readable rather than flickering every frame.
- Subscriber render latency logs include `paint_gap` and `stutters_over_threshold` to check smoothness over longer runs. Subscriber sink logs also include `replaced_before_render` to show when decoded frames were dropped because the visible render loop had not painted the previous frame yet. The stutter threshold has a 50ms floor and adapts upward for lower frame-rate runs.
- On Jetson, `--source argus` requires the Jetson Multimedia API headers under `/usr/src/jetson_multimedia_api`. It publishes NV12 DMA buffers through the Jetson hardware encoder; local publisher preview and burned timestamps are not supported on that path.
- Jetson AV1 hardware encoding requires an Orin-class device (e.g. Orin NX or AGX Orin on JetPack 5+); the encoder is probed at startup and on devices without AV1 support (e.g. Xavier) `--codec av1` automatically falls back to the software libaom encoder. The Jetson AV1 encoder produces a single L1T1 stream (no SVC).
- On Linux, preview windows use the Vulkan `wgpu` backend by default to avoid GLES/EGL conflicts on Jetson desktops. Set `WGPU_BACKEND=gl` or another supported `wgpu` backend to override this.

Latency benchmark harness:
```
# Uses target/release/publisher and target/release/subscriber.
# The run directory and room name are both target/local_video_latency/<name>.
examples/local_video/scripts/run-latency-benchmark.sh \
  --name local-sfu-h264-sw-720p-pattern \
  --url ws://127.0.0.1:7880 \
  --api-key devkey \
  --api-secret secret \
  --duration 60 \
  --test-pattern \
  --width 1280 \
  --height 720 \
  --fps 30 \
  --codec h264 \
  --encoder software
```

The harness logs publisher and subscriber output, then writes `latency-timeseries.csv`, `stutters.csv`, `receiver-stats.csv`, `worst-windows.csv`, `summary.json`, `report.md`, and `report.pdf` under `target/local_video_latency/<name>`. It defaults to `--min-playout-delay 0 --max-playout-delay 1` on the publisher and `--no-overlay --no-stats` on visible subscriber runs. Camera runs can omit `--test-pattern` and pass `--camera-index`, `--source`, and `--format`; extra raw publisher flags can be appended with repeated `--publisher-arg` entries.
On macOS, visible harness runs default to `--render-path cpu`, no-vsync presentation, and a normal backgroundable window to avoid native texture-import, focus, and always-on-top stalls observed during long latency soaks; pass `--render-path auto` when explicitly measuring the native CVPixelBuffer-to-Metal path, `--render-vsync` when comparing synchronized presentation, or `--keep-window-front` when the visible subscriber must stay focused above other windows. The bundled default-codec visible probe and soak cases opt into `--render-vsync` because it eliminated render-path stutters in a 5 minute 720p30 H.264 test-pattern run on macOS while keeping p95 latency budgets low. macOS visible subscriber runs also promote the render thread to user-interactive QoS, the harness uses `caffeinate` when available to prevent idle sleep/App Nap-style scheduling while the run is active (`--no-caffeinate` disables this), and the harness creates `.metadata_never_index` markers under `target/local_video_latency` to keep Spotlight indexing from adding benchmark noise.
Each run also includes `host-load-before.txt`, `host-load-samples.txt`, and `host-load-after.txt` snapshots; the generated summary/report includes `smoothness_status`, frame/time `coverage`, `host_load.status`, and a combined `benchmark_status` that marks short, incomplete, host-busy, or host-unknown runs. Benchmark pass requires at least 95% frame and time coverage by default; use `--min-frame-coverage-pct` or `--min-time-coverage-pct` to adjust that gate. Visible runs require render-window coverage, while headless runs only require publisher and sink-delivery coverage. Headless smoothness fails on sink-delivery stutters; visible smoothness also fails on render stutters plus decoded frames replaced or dropped before paint, so a visible run cannot pass by silently skipping frames. Reports include smoothness signal distribution fields such as the number of affected windows and the clean tail after the last signal, making it easier to distinguish one isolated blip from stutters spread across a soak. The report also includes p95 window-max timing fields for the main capture, encode, sink, render, and e2e windows so one outlier does not hide the sustained latency shape, while `worst-windows.csv` lists the exact worst timestamps for each path. Use `--wait-for-idle-host 120` to wait for a quiet host before starting; by default the preflight requires three consecutive idle samples, adjustable with `--idle-confirmation-samples`. Adjust `--host-busy-process-cpu-pct` or `--host-busy-total-cpu-pct` when a lab machine has known background load, use `--host-load-interval 0` to disable periodic sampling, or pass a larger interval to reduce sampling overhead. Use `--fail-on-stutter` to fail runs with subscriber stutters or visible frame skips, or `--require-benchmark-pass` to fail unless `benchmark_status` is `PASS`.
Optional latency-budget flags such as `--max-sink-gap-p95-ms`, `--max-e2e-p95-ms`, and `--max-encoder-upload-to-output-p95-ms` add thresholds to `summary.json` and the report. When `--require-benchmark-pass` is set, a clean run that exceeds or cannot compute an applicable configured budget fails with a latency-budget benchmark status. Headless runs still apply sink and publisher budgets, but render-window budgets such as e2e, paint gap, receive-to-decode, and receive-to-paint are reported as inapplicable instead of missing.
Pass `--decoder software` to bypass known hardware decoder backends for the subscriber (`LK_DISABLE_VIDEOTOOLBOX_DECODER` on Apple platforms and `LK_DISABLE_NVDEC` on NVIDIA systems), which is useful when isolating decoder-induced latency spikes. The analyzer marks runs with no subscriber frames as invalid so unsupported software-decoder combinations do not look like smooth passes.
Pass `--render-path cpu` to keep the same decoder but bypass native GPU texture import and force the subscriber through the CPU I420 upload path.
Pass `--headless` to isolate receiver/decode/sink delivery from the window and render loop. Headless runs are valid when sink delivery windows are present, but render latency fields will be `NA`.
Pass `--no-render-vsync` or `--render-vsync` to compare visible-window smoothness across presentation modes, or `--keep-window-front` to compare focused always-on-top behavior against the default backgroundable window.
Pass `--render-loop-diagnostics` to add render-loop scheduling windows to the logs and generated report.
Pass `--drop-late-frames-ms 250` to bound visible latency after local subscriber stalls; generated reports include the number of late decoded frames dropped before render.
By default it runs an extra 8 second warmup and excludes that startup period from the generated summaries; pass `--warmup 0` to include startup.
Use `examples/local_video/scripts/compare-latency-runs.py --refresh --all --csv target/local_video_latency/comparison.csv` to re-run the current analyzer over existing benchmark directories and rank generated runs by benchmark status, stutters, visible frame skips, p95 window-max latency, coverage, and host-load contamination.
Use `examples/local_video/scripts/run-latency-suite.py --cases examples/local_video/scripts/latency-suite.example.csv --url ws://127.0.0.1:7880 --api-key devkey --api-secret secret --overwrite` to run a CSV-defined set of named cases through the same harness. The suite CSV keeps each benchmark directory as the row's `name`, accepts columns matching the long harness flags with dashes changed to underscores, writes per-case return codes to `target/local_video_latency/<suite>-suite-results.csv`, writes an aggregate suite verdict to `target/local_video_latency/<suite>-suite-summary.json`, and includes any case with a readable `summary.json` in the aggregate CSV, Markdown, and PDF comparison reports even if `--require-benchmark-pass` made that case exit non-zero. The suite verdict fails if any case is missing a readable summary or if any readable case summary has a `benchmark_status` other than `PASS`. Use `examples/local_video/scripts/latency-suite.probe.csv` for short iteration runs that collect latency and stutter telemetry even when the host is busy; visible probe cases keep the window foregrounded to avoid measuring background render throttling. These are useful diagnostics, but host-contaminated results are not soak passes. Use `examples/local_video/scripts/latency-suite.soak.csv` for the 5 minute sustained-smoothness gate; visible soak cases also keep the window foregrounded, and all soak cases wait longer for a quiet host and fail unless the full requested duration is covered without stutters, visible frame skips, host-load contamination, or configured latency-budget misses.
Use `python3 examples/local_video/scripts/test-analyze-latency-log.py` and `python3 examples/local_video/scripts/test-run-latency-suite.py` to run the analyzer and suite-runner regression checks.
