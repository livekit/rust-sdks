# Note: Remember to update libwebrtc from https://github.com/livekit/rust-sdks/releases
cargo ndk -t armeabi-v7a -t arm64-v8a -o ./android/app/src/main/jniLibs build --release