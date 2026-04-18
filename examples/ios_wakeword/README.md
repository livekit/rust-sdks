# Wake Word Demo (iOS + macOS)

Minimal SwiftUI app that runs the [`livekit-wakeword`](../../livekit-wakeword/)
ONNX-based detector against the device microphone. Tap the button to start
listening; when the `hey_livekit` classifier score crosses 0.5 the display
turns green and shows `WAKE WORD DETECTED`.

The same SwiftUI sources build as two apps:

- **`WakewordDemo`** - iOS 15+ (iPhone, iPad, iOS Simulator)
- **`WakewordDemoMac`** - macOS 12+ native app

The macOS build is handy for iterating on the detector without a device.

## Architecture

```
Mic  ─►  AVAudioEngine tap  ─►  Float32→Int16 convert  ─►  2 s Int16 ring buffer
                                                                        │
                                                                        ▼
                                              background queue runs detector.predict()
                                                                        │
                                                                        ▼
                                                         @MainActor publishes score
                                                                        │
                                                                        ▼
                                                            SwiftUI ContentView
```

The Swift side never runs the ML inference; it just buffers audio and calls
into the UniFFI-generated `WakeWordDetector` class, which bridges to the
`livekit-wakeword` Rust crate. Inference uses the pure-Rust `ort-tract` ONNX
backend — no native dependencies are required at runtime.

## Prerequisites

- Xcode 15+ (the project currently builds with Xcode 26)
- Rust toolchain with iOS targets:

  ```sh
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```

- [`cargo-make`](https://sagiegurari.github.io/cargo-make/) (only if you want
  to use the workspace-wide `cargo make swift-package` flow; this demo builds
  the Swift package with `cargo-swift` directly — see below).
- [`cargo-swift`](https://github.com/livekit/cargo-swift) (LiveKit fork):

  ```sh
  cargo install --git https://github.com/livekit/cargo-swift.git \
      --branch feature/framework-wrapping --force
  ```

- [XcodeGen](https://github.com/yonaskolb/XcodeGen) if you want to regenerate
  the `.xcodeproj` from [`project.yml`](./project.yml):

  ```sh
  brew install xcodegen
  ```

## Build the Swift package (once)

From [`../../livekit-uniffi/`](../../livekit-uniffi/):

```sh
cd ../../livekit-uniffi
cargo swift package \
    --accept-all \
    --name LiveKitUniFFI \
    --platforms macos --platforms ios \
    --lib-type dynamic \
    --release \
    --privacy-manifest ./support/swift/PrivacyInfo.xcprivacy
mkdir -p packages/swift
rm -rf packages/swift/LiveKitUniFFI
mv LiveKitUniFFI packages/swift/
```

This produces `../../livekit-uniffi/packages/swift/LiveKitUniFFI/` with a
`Package.swift`, Swift bindings in `Sources/`, and a
`RustLiveKitUniFFI.xcframework` containing `liblivekit_uniffi.dylib` for
iOS device, iOS simulator (arm64 + x86_64), and macOS.

> The classifier model file `hey_livekit.onnx` is copied into this target's
> resources from [`../../livekit-wakeword/tests/fixtures/hey_livekit.onnx`](../../livekit-wakeword/tests/fixtures/hey_livekit.onnx).
> The mel-spectrogram and embedding ONNX models are embedded inside the
> Rust crate at compile time via `include_bytes!`.

## Run the app

Open `WakewordDemo.xcodeproj` in Xcode.

### macOS (quickest loop)

1. Pick the **`WakewordDemoMac`** scheme and **`My Mac`** as the destination.
2. `Cmd+R`. Grant microphone permission when prompted.
3. Click **Unmute mic** and say "Hey LiveKit". The score should jump toward
   1.0 and the UI flashes `WAKE WORD DETECTED`.

The macOS target is sandboxed with the hardened runtime and only the
`com.apple.security.device.audio-input` entitlement (see
[`WakewordDemo/WakewordDemoMac.entitlements`](./WakewordDemo/WakewordDemoMac.entitlements)).

### iOS

1. Pick the **`WakewordDemo`** scheme and an iOS Simulator or a connected
   device. For hardware, set the target's signing team.
2. `Cmd+R`. Tap **Unmute mic**, grant microphone permission, and say
   "Hey LiveKit".

## Regenerating the Xcode project

The Xcode project is generated from [`project.yml`](./project.yml). If you
edit the yml, regenerate with:

```sh
xcodegen generate
```

## Files

| Path                          | Purpose                                                                 |
|-------------------------------|-------------------------------------------------------------------------|
| `project.yml`                 | XcodeGen spec. Defines the iOS and macOS targets + SPM dep.             |
| `WakewordDemo/WakewordDemoApp.swift` | SwiftUI `@main` entry point (shared between iOS + macOS).       |
| `WakewordDemo/ContentView.swift`     | UI: score display, status text, mic toggle button.              |
| `WakewordDemo/WakewordEngine.swift`  | `AVAudioEngine` tap, Int16 ring buffer, background `predict()`. |
| `WakewordDemo/Resources/hey_livekit.onnx` | Wake word classifier model.                                |
| `WakewordDemo/Info.plist`            | iOS Info.plist (microphone string, orientations, scene).        |
| `WakewordDemo/Info-Mac.plist`        | macOS Info.plist (microphone string, `LSMinimumSystemVersion`). |
| `WakewordDemo/WakewordDemoMac.entitlements` | Sandbox + `audio-input` entitlement for the mac target.  |

## Tuning

Constants in [`WakewordEngine.swift`](./WakewordDemo/WakewordEngine.swift):

- `triggerThreshold` (default 0.5): score at which the UI shows a detection.
- `predictInterval` (default 0.25 s): minimum time between `predict()` calls.
- `windowSeconds` (default 2.0): size of the rolling window fed to the model.
- `triggerHoldDuration` (default 1.5 s): how long the UI stays green after a
  hit.

## Loading additional classifiers

To detect more than just "Hey LiveKit", drop more `.onnx` classifier files
into `WakewordDemo/Resources/` and add them to the `classifierPaths` array
passed to `WakeWordDetector(classifierPaths:sampleRate:)` in
`WakewordEngine.init()`, or call `detector.loadModel(path:name:)` at runtime.
