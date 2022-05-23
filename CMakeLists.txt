# TODO
# This file clearly need rework
# Build system to make it works on every platform
# Also mb automatically download prebuild libwebrtc binary so the users can directly start coding..

cmake_minimum_required(VERSION 3.22)
project(livekit-native)

set(CMAKE_BUILD_TYPE Release)
set(CMAKE_CXX_STANDARD 14)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_definitions(-DWEBRTC_POSIX=1 -DWEBRTC_MAC=1 -DGLIBCXX_USE_CXX11_ABI=0)

find_package(Boost COMPONENTS system REQUIRED)
include_directories(${Boost_INCLUDE_DIRS})

find_package(Protobuf REQUIRED)
include_directories(${Protobuf_INCLUDE_DIRS})

find_package(spdlog CONFIG REQUIRED)

include_directories(thirdparty/webrtc/include)

file(GLOB_RECURSE SRC src/*)
add_executable(livekit-native ${SRC} main.cpp)

# for cocoa
find_library(CORE_FOUNDATION Foundation)
find_library(APPLICATION_SERVICES ApplicationServices)
find_library(CORE_SERVICES CoreServices)

target_link_libraries(livekit-native PRIVATE
        spdlog::spdlog
        ${Boost_LIBRARIES}
        ${Protobuf_LIBRARIES}
        ${CORE_FOUNDATION} ${APPLICATION_SERVICES} ${CORE_SERVICES}
        ${CMAKE_CURRENT_LIST_DIR}/thirdparty/webrtc/lib/libboringssl.a
        ${CMAKE_CURRENT_LIST_DIR}/thirdparty/webrtc/lib/libwebrtc.a
)
