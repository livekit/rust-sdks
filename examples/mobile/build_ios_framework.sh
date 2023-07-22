
cargo build --release --target aarch64-apple-ios
cargo build --release --target aarch64-apple-ios-sim

xcodebuild -create-xcframework \
  -library ../target/aarch64-apple-ios/release/libmobile.dylib \
  -headers ./include/ \
  -library ../target/aarch64-apple-ios-sim/release/libmobile.dylib \
  -headers ./include/ \
  -output ios/MobileExample.xcframework