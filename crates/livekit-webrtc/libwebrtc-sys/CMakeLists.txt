# IMPORTANT NOTE
# This file is just used because some IDEs need to understand how to do autocompletion.
# This file is completely ignored by the library ( See build.rs for the build system )

cmake_minimum_required(VERSION 3.22)
project(livekit-webrtc)

set(CMAKE_CXX_STANDARD 17)

add_definitions(-DWEBRTC_WIN)

include_directories(libwebrtc/include)
include_directories(libwebrtc/include/third_party/abseil-cpp/)
include_directories(libwebrtc/include/third_party/libc++/)
include_directories(include/)
include_directories(../../../target/cxxbridge) # Can be different

file(GLOB_RECURSE SRC src/*.cpp)
add_library(livekit-webrtc ${SRC})

#include_directories(/Users/theomonnom/Library/Android/sdk/ndk/25.0.8775105/toolchains/llvm/prebuilt/darwin-x86_64/sysroot/usr/include)
#find_library(ANDROID_LIB_ANDROID android)
#target_link_libraries(client_sdk_native PRIVATE android)