cmake_minimum_required(VERSION 3.22)
project(livekit-native)

set(CMAKE_CXX_STANDARD 17)

find_package(Boost COMPONENTS system REQUIRED)
include_directories(${Boost_INCLUDE_DIRS})

find_package(Protobuf REQUIRED)
include_directories(${Protobuf_INCLUDE_DIR})

file(GLOB_RECURSE SRC src/*)
add_executable(livekit-native ${SRC} main.cpp)
target_link_libraries(livekit-native ${Boost_LIBRARIES} ${Protobuf_LIBRARIES})