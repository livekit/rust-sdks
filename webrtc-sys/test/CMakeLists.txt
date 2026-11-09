cmake_minimum_required(VERSION 3.10)
project(webrtc_sys_tests LANGUAGES C CXX ASM)

set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")

add_subdirectory(benchmark)
