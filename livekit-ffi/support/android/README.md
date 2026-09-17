# LiveKit UniFFI — Android library

Gradle Android library module that packages UniFFI Kotlin bindings and native `liblivekit_uniffi.so` binaries into an **AAR**.

## Prerequisites

- Android SDK (via Android Studio or `ANDROID_HOME`)
- Android NDK (installed via Android Studio or `ANDROID_NDK_HOME`)

## Full package build

From the crate root:

```bash
cargo make android-package                              # debug .so in release AAR
cargo make --profile release android-package            # release .so (CI / publishing)
```

The built AAR is located at: `packages/android/livekit-uniffi-android-release.aar`

In the default (development) profile, Rust builds are unoptimized debug artifacts;
`--profile release` additionally applies release Rust flags and runs the size gate.

### Local Dev

```bash
cargo make android-package-local
```

Builds the release AAR (with debug or release `.so` files per profile above) and
publishes it to the local Maven repo for app integration.

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
    implementation("io.livekit:livekit-uniffi-android:x.y.z")
}
```

The AAR bundles `jniLibs` and compiled Kotlin. UniFFI also requires **JNA** and **kotlinx-coroutines**; this module declares them as `implementation` dependencies so they are pulled in transitively when consuming through Maven.

### Local app development

Add `mavenLocal()` to your app's repository list (before `mavenCentral()` so that the local artifact wins):

```kotlin
// settings.gradle.kts
dependencyResolutionManagement {
    repositories {
        mavenLocal()
        google()
        mavenCentral()
    }
}
```

If your project uses per-module `repositories` in `build.gradle.kts` instead, add `mavenLocal()` there in the same order.

Depend on the artifact as above, using the `VERSION_NAME` in this module's `gradle.properties`:

```kotlin
dependencies {
    implementation("io.livekit:livekit-uniffi-android:0.0.1")
}
```

Re-run `cargo make android-package-local` to rebuild and publish any changes.
