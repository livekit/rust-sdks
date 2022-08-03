# IMPORTANT NOTE
# This file is just used because some IDEs need to understand how to do autocompletion.
# This file is completely ignored by the library ( See build.rs for the build system )

cmake_minimum_required(VERSION 3.22)
project(client_sdk_native)

set(CMAKE_CXX_STANDARD 14)

add_definitions(-DWEBRTC_POSIX -DWEBRTC_MAC)

include_directories(include/)
include_directories(libwebrtc/include)

# Fake library
add_library(client_sdk_native src/peer_connection_factory.cpp)