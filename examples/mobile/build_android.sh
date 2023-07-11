cargo ndk -t armeabi-v7a b --release
cargo ndk -t arm64-v8a b --release

mkdir -p android/app/src/main/jniLibs/armeabi-v7a
mkdir -p android/app/src/main/jniLibs/arm64-v8a

mv ../target/armv7-linux-androideabi/release/libmobile.so android/app/src/main/jniLibs/armeabi-v7a/libmobile.so
mv ../target/aarch64-linux-android/release/libmobile.so android/app/src/main/jniLibs/arm64-v8a/libmobile.so