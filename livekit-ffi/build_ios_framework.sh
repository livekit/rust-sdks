export LK_CUSTOM_WEBRTC=`pwd`/webrtc-sys/libwebrtc/ios-device-arm64-release
cargo build --release --target aarch64-apple-ios
export LK_CUSTOM_WEBRTC=`pwd`/webrtc-sys/libwebrtc/ios-simulator-arm64-release
cargo build --release --target aarch64-apple-ios-sim

xcodebuild -create-xcframework \
  -library ../target/aarch64-apple-ios/release/liblivekit_ffi.a \
  -library ../webrtc-sys/libwebrtc/ios-device-arm64-release/lib/libwebrtc.a \
  -headers ./include/ \
  -library ../target/aarch64-apple-ios-sim/release/liblivekit_ffi.a \
  -library ../webrtc-sys/libwebrtc/ios-simulator-arm64-release/lib/libwebrtc.a \
  -headers ./include/ \
  -output ios/LiveKitFFI.xcframework

#export LK_CUSTOM_WEBRTC=`pwd`/webrtc-sys/libwebrtc/ios-device-arm64-release