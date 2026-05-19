# LiveKit UniFFI — Android library

Gradle Android library module that packages UniFFI Kotlin bindings and native `liblivekit_uniffi.so` binaries into an **AAR**.

## Prerequisites

- Android SDK (via Android Studio or `ANDROID_HOME`)
- Android NDK (installed via Android Studio or `ANDROID_NDK_HOME`)

## Full package build

From the crate root:

```bash
cargo make android-package
```

This runs, in order: `build-android-platforms` → `bindgen-kotlin` → `android-copy-so` → `android-assemble` → `android-copy-to-packages`.

Published artifact: `packages/android/livekit-uniffi-android-release.aar`

## Step by step

```bash
cargo make build-android-platforms
cargo make bindgen-kotlin
cargo make android-copy-so
```

## Kotlin sources

`build.gradle.kts` automatically adds `../../packages/kotlin` as a source root when that directory exists (output of `cargo make bindgen-kotlin`). No manual copy is required for Kotlin.

## Build AAR

From the crate root:

```bash
cargo make android-assemble
```

Or from this directory:

```bash
./gradlew assembleRelease
```

Output:

```
build/outputs/aar/livekit-uniffi-android-release.aar
```

## App integration

```kotlin
dependencies {
    implementation(project(":livekit-uniffi")) // or published Maven coordinate
}
```

The AAR bundles `jniLibs` and compiled Kotlin. UniFFI also requires **JNA** and **kotlinx-coroutines**; this module declares them as `implementation` dependencies so they are pulled in transitively.
