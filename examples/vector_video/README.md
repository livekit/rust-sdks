# Vector video over LiveKit Data Tracks

The publisher streams a camera or file over a LiveKit Data Track. Choose `--mode layered-polygon` (the default) for overlapping flat-colored shapes or `--mode colored-polygon` for a triangle mesh. Both modes default to 15 fps. Use `--fps 30` to request 30 fps. The subscriber reads the mode and cadence from the stream and opens a playback window.

## Run with a camera

Camera capture uses the same Nokhwa backends as `local_video` on macOS, Linux, and Windows. The subscriber opens a native desktop window using egui and wgpu. Use a LiveKit server with Data Tracks enabled. Each binary signs its own participant token locally with the grants it needs.

Replace the URL, API key, and API secret in both commands. Start the subscriber first. Use the same `--room-name` on both processes and give them distinct `--identity` values.

```sh
cargo run -p vector_video --bin vector_video_subscriber -- \
  --url wss://livekit.example.com --api-key YOUR_API_KEY --api-secret YOUR_API_SECRET \
  --room-name vector-video --identity operator --publisher robot-camera
```

Then start the camera publisher in the other terminal.

```sh
cargo run -p vector_video --bin vector_video_publisher -- \
  --url wss://livekit.example.com --api-key YOUR_API_KEY --api-secret YOUR_API_SECRET \
  --room-name vector-video --identity robot-camera --camera-index 0
```

The commands show the default room and identities. `--publisher` filters by the remote publisher's identity. The flags can also read from `LIVEKIT_URL`, `LIVEKIT_API_KEY`, and `LIVEKIT_API_SECRET`. If you supply neither API credential, `LIVEKIT_TOKEN` can provide a pre-issued token with its own room, identity, and grants. Both endpoints need permission to publish data for the peer timing RPC.

Both binaries default to the `vector-video-v1` track. Use the same `--track` on both for a custom name. Pass `--list-cameras` to the publisher to enumerate devices without connecting to LiveKit. The default camera request is 640×480 at 15 fps. Set `--width` and `--height` to choose the video resolution and request the same dimensions from the camera. Set `--fps` to choose the cadence. If the camera rejects the request, the publisher tries alternate modes. It prefers an advertised mode that can deliver the requested frame rate and logs the negotiated format. Capture faster than the requested rate is decimated in the camera worker. It fits the image to the requested canvas with centered padding. The default canvas is 384×216. If capture falls behind, it drops frames to keep the queue bounded. The default Cargo development build optimizes this example’s capture, codec, and renderer. On macOS, allow camera access for the app launching the publisher.

For **720p colored polygons**, add `--mode colored-polygon --width 1280 --height 720` to the publisher command. A single dimension infers a 16:9 canvas, so `--height 720` is sufficient. The subscriber uses the stream dimensions to fit the triangles to the window and draws them directly on the GPU. MP4 recordings rasterize the geometry at the stream dimensions. Both dimensions must be even for MP4 recording.

Use `--output received.mp4` on the subscriber to record the reconstruction instead of opening the live window. Recording requires FFmpeg with `libx264`. Frame gaps are logged by the subscriber. The last rendered frame stays visible while it waits for another picture.

## Run without a camera

A file or URL can supply the publisher with `--input`. Install FFmpeg for this input. The publisher converts the file to a constant frame rate of 15 fps by default or uses an explicit `--fps` value. To try the example with test video, start the subscriber as above and run these commands in the publisher terminal.

```sh
ffmpeg -f lavfi -i testsrc2=size=640x360:rate=30 -t 8 -c:v libx264 input.mp4
cargo run -p vector_video --bin vector_video_publisher -- --input input.mp4
```

Supply the same connection flags or environment variables as in the camera example. To exercise live timing without a camera or a video file, run the publisher with `--fps 30 --input-format lavfi --input 'testsrc2=size=640x360:rate=30,realtime'`. This generates new source frames in real time. Press Ctrl-C to end the stream.

Use the offline preview to see the source, reconstruction, and decoded outlines over the source, from left to right.

```sh
cargo run -p vector_video -- preview input.mp4 comparison.mp4
```

Add `--drop-every 5` to simulate losing every fifth message. The preview keeps the last received image during the gap and resumes as soon as it receives a decodable picture. Use `--compression spatial` to make every message independently decodable.

## Colored polygon mode

Add `--mode colored-polygon` to the publisher. The subscriber detects wire version 6 automatically. Both the live window and the MP4 recorder render the same filled polygons. To compare the source, reconstruction, and mesh alignment locally, run the following command.

```sh
cargo run -p vector_video -- preview input.mp4 polygons-720p.mp4 --mode colored-polygon --height 720
```

At 720p the preview writes a 3840×720 comparison with three 1280×720 panels. The subscriber recording contains one 1280×720 picture per frame.

Colored polygon mode defaults to **15 fps and a maximum of 432 visible triangles per frame**. Set `--max-triangles` on the publisher, preview, or benchmark to control detail. This flag also selects colored-polygon mode. For a 720p stream with up to 10,000 triangles at 30 fps, add `--height 720 --fps 30 --max-triangles 10000` to your publisher command. Rebuild both publisher and subscriber for dense meshes.

```sh
cargo run -p vector_video -- preview input.mp4 polygons-720p.mp4 --height 720 --fps 30 --max-triangles 10000
```

The encoder sizes a stable base grid from the triangle limit and adds subdivisions where they preserve detail. Each split replaces one triangle with four. At a limit of 500, the feature encoder uses 288 movable-grid triangles and can split 70 of them, giving 498 visible triangles. The minimum setting is two triangles. The publisher logs the mesh capacity at startup. The actual count can be lower on smooth scenes or when the analysis raster runs out of detail.

Add `--merge-flat-regions` to give adjacent low-contrast triangles one shared color and remove subdivisions inside the merged area. The encoder bounds the color variation across the whole group, so small differences cannot chain across a larger gradient. The existing mesh geometry and packet format carry the result, so subscribers can decode it without an upgrade.

Add `--low-bandwidth` to use the sparse triangle fit with mesh-patch compression. The publisher uses a coarser grid, fewer subdivisions and up to 12 shared palette colors. It usually reuses the layout for six changed pictures and can refresh earlier after a large scene change. Between refreshes it prioritizes triangle color changes that reduce image error the most. Payload varies with scene motion and complexity. The stream keeps its requested frame rate and canvas dimensions.

Mesh-patch sends short lists of triangle color changes, including a five-byte repeat for an identical mesh. At 30 fps a receiver with the independent reference can recover from a lost delta within 12 pictures if the next recovery packet arrives. An independent snapshot starts a new reference chain at least once per second. Use a subscriber with mesh-patch support, or specify another wire format with `--compression`.

Older commands using `--low-bandwidth=sparse`, `=low` or `=minimum` select the same sparse fit. The former `=10kbps` setting enforced a fixed payload budget. To switch to variable-rate sparse encoding, replace it with `--low-bandwidth`.

Colored-polygon mode uses the triangle limit to bound each snapshot. Payload bitrate varies with the triangle count, scene, cadence and compression. The subscriber overlay shows the measured payload bitrate.

The encoder fits persistent triangle vertices at half the input width and height. It analyzes 192×108 samples at the default canvas and 640×360 samples at 720p. Small canvases use the default analysis raster. The four children share exact edge midpoints and use the frame's 16-entry RGB565 palette, so a subdivision uses the existing vertex coordinates. Wire compression then codes that geometry, palette and set of refinements.

The publisher, preview and benchmark default to `--detail-priority features`. The encoder estimates local edge directions from luma gradients on 4×4 analysis tiles. A strong gradient in one direction identifies a line. Strong variation in two directions raises the priority of a corner or junction. Multiscale RGB contrast also picks up color boundaries. These weights influence vertex placement, fitted colors and which triangles split. The encoder reserves roughly 60% of the requested count for movable base geometry and smooths weights over time. Eyes, brows and silhouettes can benefit from these cues. A small or low-contrast face can still lose detail. [Structure tensors and Shi–Tomasi corners](https://docs.opencv.org/4.x/d4/d8c/tutorial_py_shi_tomasi.html)

`--detail-priority structure` uses multiscale contrast ranking to allocate refinements. `--detail-priority color-error` fits unweighted RGB reconstruction error. All three priorities produce the same polygon packet format. [Multiscale visual saliency](https://ilab.usc.edu/bu/theory/index.html)

Vertex motion is limited to one eighth of a cell per axis per frame. A split used in the last frame gets a 25% preference when the encoder ranks refinements. Low-noise stationary inputs reuse the previous picture exactly. A large scene-wide color change resets tracking. Fine detail can still disappear when it lies below the mesh resolution.

`--fps` sets the polygon cadence independently of the triangle limit. If a native camera negotiates a faster mode, the capture worker selects frames at the requested rate before resizing. A slower camera streams at its negotiated rate.

For native colored-polygon capture at 1080p, the camera worker sends a 960×540 analysis image to the mesh fitter. Packed YUYV capture uses GPU conversion when a hardware adapter is available and falls back to CPU conversion. The stream still advertises 1920×1080. At 30 fps and a 500-triangle limit, check the publisher's send rate and the subscriber's `Window` rate separately when diagnosing dropped frames.

## Overlapping polygon mode

`--mode layered-polygon` is the default and encodes an opaque background and a back-to-front list of independently shaped, flat-colored polygons. A polygon can cover any part of an earlier one. The publisher and preview default to a maximum of 48 polygons per frame, excluding the background. Override the budget with `--max-layers`. Cadence defaults to 15 fps and is set separately with `--fps`. The live subscriber draws layered polygons directly on the GPU. Preview and MP4 recordings rasterize them at the stream resolution.

```sh
cargo run -p vector_video -- preview input.mp4 layered.mp4 --height 720 --fps 30 --max-layers 48
```

The default encoder shortlists connected color regions on a small grid, fits the selected contours at a higher resolution and reuses outlines when they still match the scene. Each contour has at most eight vertices. Use `--feature-layers` to give compact details more weight. Use `--detailed-layers` for the previous multi-scale fitting path when fine detail has priority over encode time. Both paths use the same decoder and layer budget.

In local 1080p30 tests with SDNet on the CPU, the default 48-layer search encoded all 2,264 frames in 5 ms or less and peaked at 4.52 ms. The publisher prints mean and peak encode time and the count above 5 ms at shutdown. The timing includes packet packing. Live model inference runs on a separate worker.

Add `--line-strokes` to represent thin regions with a constant-width open path of two or three points. The fitter checks the selected image contours after fitting and replaces qualifying contours in place. A stroke occupies the same layer slot and 18-byte record as a filled contour. Both recording and GPU playback use the same centerline geometry. Use an updated subscriber with this option.

With `--detailed-layers --feature-layers`, the fitter flattens smooth surfaces to light and shadow tones and traces compact marks on the 384×216 analysis grid. It compares alternative outlines after coordinate rounding and can give up to a third of the layers to fine details. In textured scenes it adjusts broad outlines against the colors on both sides of a boundary and tracks fine details between frames. Combine this with `--object-layers` for model-derived silhouettes.

After the fine pass, the fitter can strengthen weak brightness differences between neighboring fills when the visible boundary agrees with the source. It adjusts small polygons while keeping the broad light and shadow fills flat.

In smoother scenes where many features are already resolved on the coarse grid, compact coarse outlines get more of the layer budget. The fitter still reserves room for finer marks.

```sh
cargo run -p vector_video --bin vector_video_publisher -- \
  --mode layered-polygon --feature-layers --max-layers 48 --fps 30 --height 1080
```

Without `--face-layers`, native capture sends a 192×108 RGB analysis plane when the stream canvas is at least that large. With `--feature-layers`, a canvas of at least 384×216 keeps the denser sample grid for fine features. For packed YUYV at the requested resolution, capture converts only the pixels needed by the analysis grid. The stream advertises the selected canvas size.

Add `--face-layers` on a macOS, Linux, or Windows publisher to reserve slots for up to four faces. The publisher runs BlazeFace and a 478-point MediaPipe Face Landmarker with the Rust tract CPU backend. Both models ship with the example and work offline. With enough slots, each face gets a silhouette and separate polygons for the nose, lips, eyes, brows and irises. Only the resulting colored polygons travel on the Data Track. [Model sources and license](models/README.md)

The live publisher starts with image-derived regions and incorporates facial outlines when the detector returns. It allows one inference in flight, tracks local motion between observations and discards pending landmarks after a scene cut. Detector speed affects how frequently the outlines refresh. Offline preview and benchmark wait for each scheduled observation so the same input produces repeatable codec comparisons.

### Model-guided object outlines

Add `--object-layers` to guide layered polygons with the bundled U²-NetP model. It segments salient foreground objects on a 160×160 input. The encoder traces their silhouettes, then fits foreground colors and details inside those outlines. A complex silhouette can use several eight-vertex polygons. With `--feature-layers`, the encoder compares how well candidate polygons cover each part after quantizing their coordinates. In textured scenes it also gives useful marks within those outlines a share of the available detail slots. Object contours and marks share the existing `--max-layers` budget. The subscriber receives ordinary layered polygon records and needs no model or runtime. [Model and license](models/README.md)

With `--detailed-layers --feature-layers`, the color fit can extend to a stronger source edge just beyond a model boundary that crosses a nearly uniform color.

In a textured scene with a persistently smoother foreground, `--feature-layers` can give four broad-fill slots to fine details within the same layer budget.

The final four fine layers can favor compact polygons that recover missing source contrast on a guided subject. A candidate scores lower if it would paint over an edge that is already visible. The fine pass traces thin lines in overlapping tiles and checks how much of each simplified outline still covers the source line. It also traces color marks across small changes in brightness, including where the draft fill differs slightly from the source.

Install the ONNX Runtime 1.23 or newer shared library for the publisher's operating system and architecture. Set `ORT_DYLIB_PATH` to the full path to the runtime library if it is outside the platform's library search path. On macOS, an ONNX Runtime build with Core ML support lets the publisher run segmentation on Apple's CPU and GPU. Linux and Windows use the CPU, as does macOS when Core ML is unavailable. [ONNX Runtime releases](https://github.com/microsoft/onnxruntime/releases) · [Core ML provider](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html)

Add `--object-model /path/to/model.onnx` with `--object-layers` to test a different foreground model using ONNX Runtime CPU on any supported platform. The graph must accept one fixed float32 `[1, 3, H, W]` tensor of ImageNet-normalized RGB and return one float32 `[1, 1, H, W]` foreground probability mask. The same option works in preview and benchmark. Pass `models/u2netp.onnx` from the example directory to measure the bundled model on the same CPU path.

```sh
cargo run -p vector_video --features object-layers --bin vector_video_publisher -- \
  --object-layers --feature-layers --camera-index 0 --width 1920 --height 1080 --fps 30 --max-layers 48
```

The live publisher runs one inference at a time and tracks the contours between results. It usually waits three changing frames between requests. With `--feature-layers`, parts of a split silhouette can move independently when their source pixels match a nearby position better than the whole silhouette's shift. When a large frame change has consistent motion across the image, the publisher keeps tracking through the camera pan and can refresh Apple Vision after two changing frames. Fast subject motion, occlusion, and low salience can still obscure a subject. Use the same feature and flag with `preview` for a local comparison, or with `benchmark` to compare payload rates.

On macOS 14 or later, `--object-provider apple-vision` selects the system foreground instance model. Build with the macOS 14 SDK or later and use `--feature-layers` to give the model the 384×216 analysis image. Apple Vision handles the outlines on the publisher and runs without ONNX Runtime. The optional `--part-model` still uses ONNX Runtime for fine detail. [Apple Vision foreground masks](https://developer.apple.com/documentation/vision/vngenerateforegroundinstancemaskrequest)

```sh
cargo run -p vector_video --features apple-vision --bin vector_video_publisher -- \
  --object-layers --object-provider apple-vision --feature-layers \
  --camera-index 0 --width 1920 --height 1080 --fps 30 --max-layers 48
```

When Apple Vision covers a rectangular video inset with one foreground mask, `--feature-layers` checks the inset's visible borders and runs one extra segmentation pass inside it. Any resulting silhouettes share the same layer budget. On the 157-frame wide shot of two people in an inset, the extra pass retained 26% of eye edges and 36% of mouth edges, up from 17% for each. The localhost publisher averaged 30.14 fps at 48 layers and used 125 kbps of Data Track payload with `compact-motion`.

### Contextual detail guidance

Pass `--part-model /path/to/parts.onnx` with `--feature-layers` to compare the source and draft polygons with a contextual model. When the model identifies missing structure, the fitter tests small polygons against the source colors and edges within the existing layer budget. The publisher refreshes guidance every six changing frames and keeps at most one inference in flight. `--object-layers` can guide the outlines in the same stream. [Model export](models/README.md#part-guidance)

A part model can also return foreground probabilities in that same pass. The fitter gives those areas more weight and uses source colors and edges to find their boundaries. This can help recover shapes that the segmentation model misses.

On an Apple M4 Pro, the combined model took a median of 30 ms per observation with the Core ML execution provider and 88 ms with the CPU provider. Both the face and object replays held 30 fps in a 1080p localhost LiveKit stream at 48 layers. The publisher logs the provider it selects.

In four matched 1080p30 localhost replays at 48 layers with Apple Vision and `compact-motion`, the optional contextual model improved eye-edge retention from 26% to 35% in a wide shot and wheel-edge retention from 37% to 39% on a bicycle. Mouth-edge retention in the wide shot fell from 36% to 32%. All 698 frames arrived with or without contextual guidance. Total publisher CPU time including startup was 29.9 seconds with image-derived detail, 36.9 seconds with the contextual model on ONNX Runtime CPU, and 41.7 seconds with a quantized variant that selected the Core ML provider.

```sh
cargo run -p vector_video --features object-layers --bin vector_video_publisher -- \
  --object-layers --feature-layers --part-model /path/to/parts.onnx \
  --camera-index 0 --width 1920 --height 1080 --fps 30 --max-layers 48
```

## Lossless wire compression

The publisher and preview select mesh-patch compression with `--low-bandwidth` and predictive compression otherwise. An explicit `--compression` overrides either default. The visual encoder chooses shapes that fit the requested budget. For layered polygons, the predictor can change the starting vertex and winding to match the reference more closely. Compression preserves all geometry, colors and frame indices and adds zero frames of lookahead. The subscriber detects the packet format automatically.

| Publisher setting | What travels over the Data Track | Recovery after loss |
| --- | --- | --- |
| `--compression off` | Original self-contained snapshot | Next message |
| `--compression spatial` | Smallest of the original snapshot and an independently entropy-coded snapshot | Next message |
| `--compression predictive` | Smallest of those two and a delta against a recent reference. An exact repeat takes seven bytes. | Display holds until a usable reference arrives |
| `--compression compact` | Also tries a delta that skips unchanged layers or mesh faces and favors small changes to colors and coordinates. | Same refresh policy as predictive |
| `--compression mesh-patch` | Sends sparse lists of triangle color changes and tries a mesh predictor for geometry and palette refreshes. | Same refresh policy as predictive with the same mesh settings |
| `--compression compact-motion` | Also tries a layered delta that can reuse a polygon at a nearby drawing position and send a new origin for a translated outline. | Same refresh policy as predictive |

Compact mode requires an updated subscriber and adds no frame of buffering. It tries both temporal encodings and chooses the smaller packet for the current reference. Each mesh base vertex uses a single byte for both signed offsets. Indexed faces use four-bit palette indices and the palette stores RGB565 colors. Floating-point values used in fitting and rasterization stay local.

Rebuild the subscriber before using `--compression compact-motion`. This setting uses compact compression for colored polygons.

At 1080p30 with `--low-bandwidth` and a 400-triangle cap, mesh-patch reduced aggregate payload from 17.28 to 15.47 kbps across 3,050 frames from 37 clips. At a 500-triangle cap it reduced payload from 20.66 to 18.66 kbps on 510 frames from six clips. The standard 500-triangle fit on the same clips went from 53.07 to 52.13 kbps. The benchmark verified every reconstructed snapshot byte for byte.

For triangle meshes, the encoder predicts vertex offsets, RGB565 palette components, triangle colors, subdivisions and child colors separately. Nearby regions and the previous mesh supply predictors. Each snapshot codes the prediction residuals with a local adaptive rANS coder. Layered-polygon predictive compression matches bytes by position across snapshots. Compact compression can also code RGB565 components and signed changes to vertex offsets by layer. Both can align the starting vertex and winding of a closed outline to the reference while preserving its edges. [rANS coding notes](https://fgiesen.wordpress.com/2014/02/02/rans-notes/)

At 15 fps the publisher sends an independent refresh at least every 15 polygon frames. Within a refresh interval, every fourth frame can decode directly from the independent reference. Losing an ordinary delta can therefore affect up to four pictures. A late join or lost independent reference can require waiting for the next refresh. More losses can extend either interval.

### Compare bitrate on your footage

```sh
cargo run -p vector_video -- benchmark input.mp4 --mode colored-polygon --height 720
```

The benchmark reports average and peak triangle or layer counts, visual encoding time, and rendering time. It runs each applicable compression method over identical visual encoder output, verifies that every decoded snapshot matches the original bytes, and reports payload, modeled transport rate, and compression time. The following runs used 1280×720, 15 fps, a maximum of 432 visible triangles, and one message per frame.

| Clip | Off (kbps) | Spatial (kbps) | Predictive (kbps) | Payload saved |
| --- | ---: | ---: | ---: | ---: |
| Aerial pan, 120 frames | 46.80 | 40.59 | 22.61 | 52% |
| Moving mug, 120 frames | 39.95 | 28.92 | 12.27 | 69% |
| Moving test pattern, 120 frames | 37.91 | 26.32 | 16.20 | 57% |
| Unrelated photo cuts, 50 frames | 46.63 | 39.68 | 39.15 | 16% |
| Frozen aerial frame, 120 frames | 46.80 | 39.96 | 3.45 | 93% |

## Frame rate and latency

The publisher logs capture FPS, queue drops, encoder time, and rolling modeled rate. The subscriber logs receive, reconstruction and window paint rates separately. The native window takes the latest picture at each repaint and counts superseded pictures. Use `--max-display-fps 25` on the subscriber to reproduce a display slower than a 30 fps camera. Cameras pace themselves. File playback uses absolute source deadlines and a separate minimum spacing for catch-up sends.

The live overlay reads `30.0 FPS * 44.1 kbps * 82.3 ms g2g`. FPS counts newly painted pictures, and kbps counts received Data Track payload bytes over the sampling interval. The g2g field refreshes twice per second. It shows the arithmetic mean of valid camera exposure-to-GPU completion times for frames completed in the preceding second. Each frame contributes one sample. Display scanout and panel response occur after this software endpoint. Hover over the overlay to see the most recent subscriber–publisher round trip. RTT is also printed by the subscriber.

Native capture takes the timestamp from the same Nokhwa camera buffer and Unix-microsecond validation used for `local_video`’s video-track `user_timestamp`. Its proximity to the physical exposure depends on the camera and driver. Every fragment of a picture carries the same Data Track `user_timestamp`.

The subscriber periodically probes the publisher over LiveKit RPC and uses the lowest-RTT recent sample to map subscriber monotonic time into the publisher’s clock. The estimate assumes roughly symmetric travel times, so different forward and return delays can bias g2g. The overlay shows `— ms g2g` when the averaging window has no valid samples or the clock calibration is unavailable. FFmpeg inputs have no camera exposure timestamp. Single-picture snapshots carry exposure time entirely in Data Track metadata.

### Measure per-frame latency

Add `--latency-csv publisher.csv` to the publisher and `--latency-csv subscriber.csv` to the subscriber. Use new filenames for each run. Enabling CSV adds no network traffic. Comparisons of raw host wall-clock events such as `capture` to `paint` require synchronized clocks. Each exposure-to-GPU trace measurement uses the RPC clock mapping and retains the individual frame’s latency.

```sh
python3 examples/vector_video/latency.py publisher.csv subscriber.csv --frames joined.csv
```

The report joins events by source frame ID, counts missing stages, and reports percentiles, the first and last ten-second medians, and a linear trend in ms/min. The joined CSV retains startup and missing frames. The summary excludes the first second by default. Use Python 3.10 or later.

| Event | Timestamp |
| --- | --- |
| `capture` | Camera backend returned pixels, before conversion or pipe buffering. For FFmpeg inputs, the first RGB byte reached the publisher. Camera hardware or upstream decoder buffering precedes this timestamp. |
| `exposure` | Camera-buffer timestamp in publisher Unix microseconds. Recorded on both peers when available. |
| `send` | Publisher enqueued the Data Track message. |
| `receive` | Subscriber obtained the Data Track message, before decoding. |
| `render` | Reconstruction completed at the subscriber. |
| `paint` | The live window submitted the new picture to egui. A superseded frame has a render event and no paint event. GPU presentation, display scanout, and exposure are outside this measurement. |
| `gpu_publisher_clock` | GPU completion observed by the native viewer and mapped to publisher Unix microseconds. Subtract `exposure` to obtain the overlay measurement. |
| `file` | The raw picture was written to the FFmpeg recorder. |

### Payload layout

One Data Track message normally carries a complete snapshot. The publisher divides snapshots larger than 15,000 bytes into parts and spaces those parts across one source frame interval to avoid overflowing the Data Track send queue. Each part carries `CF`, the frame index, total snapshot length and zero-based part index (three little-endian `u32` values), followed by up to 15,000 bytes of snapshot data. The subscriber collects the parts before decoding. Version 6 meshes use a one-byte grid preset or `0xff` followed by the column and row counts (two little-endian `u16` values). Older subscribers support only the preset grids.

An uncompressed snapshot starts with `CV`, a format version, the source frame index (`u32`), FPS numerator and denominator (`u16` each), and a frame count of one (`u8`). Multi-byte integers are little-endian. Version 6 carries colored-polygon meshes and version 7 carries layered polygons. For a custom canvas, bit 7 of the version byte is set and the frame count is followed by the width and height (`u16` each). Those four bytes count toward the payload budget. An independently compressed snapshot (`CI`) retains the same header fields. A predictive delta (`CD`), compact delta (`CT`), or repeat (`CR`) carries a four-byte frame index and one-byte reference distance after its magic, and inherits the reference's format, dimensions, frame count and cadence.

Mesh-patch independent snapshots (`CK`) and dense deltas (`CX`) use the `CI` and `CD` header layouts. When a dense delta references the immediately preceding mesh, a single `D`, `T` or `X` byte selects the `CD`, `CT` or `CX` body, followed by the absolute `u32` frame index. The mesh-specific predictors signal whether vertex displacements are all even and how many leading palette entries to transmit. The decoder reconstructs any trailing copies of palette slot zero and uses the green-channel change to predict the red and blue channels. A short color update starts with `0x80 | reference_distance` followed by the absolute little-endian `u32` frame index. Its body carries an unsigned LEB128 edit count, then Golomb–Rice gaps between edited triangles and truncated-binary replacement palette indices. The triangle order visits each base color followed by its three corner colors when subdivided. The Rice remainder width is the integer floor of log2(visible triangles / edit count). Replacement indices omit the previous slot and trailing copies of slot zero. A repeat uses `0xc0 | reference_distance` and the same frame index with an empty body. Reference distances range from 1 to 60 frames.

Version 7 carries a two-byte RGB565 background, followed by 18 bytes per polygon in paint order. Each record contains an RGB565 fill and eight coordinate pairs on a normalized 0–255 plane. The first pair is absolute and the remaining pairs are byte differences modulo 256. Repeating the last vertex pads a shorter contour. Packet length determines the layer count. The receiver uses even-odd fill and selects the frontmost polygon at each pixel. Both predictive and compact deltas carry the restored body length as a little-endian `u16` before the entropy-coded data.


