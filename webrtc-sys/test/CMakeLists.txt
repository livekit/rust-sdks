cmake_minimum_required(VERSION 3.25)
project(webrtc_sys_tests LANGUAGES C CXX ASM)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)
set(CMAKE_RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}")

if(NOT CMAKE_BUILD_TYPE)
  set(CMAKE_BUILD_TYPE Release)
endif()

set(WEBRTC_ROOT "" CACHE PATH "Path to the extracted LiveKit libwebrtc artifact")
if(NOT WEBRTC_ROOT)
  message(FATAL_ERROR "WEBRTC_ROOT must point to an extracted LiveKit libwebrtc artifact")
endif()

set(CUDA_HOME "$ENV{CUDA_HOME}" CACHE PATH "Path to the CUDA toolkit")
if(NOT CUDA_HOME)
  set(CUDA_HOME "/usr/local/cuda")
endif()

set(WEBRTC_INCLUDE "${WEBRTC_ROOT}/include")
set(WEBRTC_LIBCXX "${WEBRTC_INCLUDE}/third_party/libc++/src/include")
set(WEBRTC_LIBCXXABI "${WEBRTC_INCLUDE}/third_party/libc++abi/src/include")
set(WEBRTC_SYS_SRC_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../src")

set(WEBRTC_COMPILE_DEFINITIONS)
foreach(ninja_file webrtc.ninja desktop_capture.ninja)
  file(STRINGS "${WEBRTC_ROOT}/${ninja_file}" ninja_defines_line LIMIT_COUNT 1)
  string(REGEX MATCHALL "-D[A-Za-z_][A-Za-z0-9_]*(=[^ ]+)?" ninja_defines
    "${ninja_defines_line}")
  foreach(ninja_define IN LISTS ninja_defines)
    string(REGEX REPLACE "^-D" "" ninja_define "${ninja_define}")
    if(NOT ninja_define MATCHES "^(CR_CLANG_REVISION|CR_XCODE_VERSION)=")
      list(APPEND WEBRTC_COMPILE_DEFINITIONS "${ninja_define}")
    endif()
  endforeach()
endforeach()

find_package(Threads REQUIRED)

function(configure_webrtc_test_target target)
  target_compile_definitions(${target} PRIVATE
    ${WEBRTC_COMPILE_DEFINITIONS}
    USE_NVIDIA_VIDEO_CODEC=1
  )
  target_include_directories(${target} PRIVATE
    "${WEBRTC_SYS_SRC_DIR}"
    "${WEBRTC_SYS_SRC_DIR}/nvidia/NvCodec/include"
    "${WEBRTC_SYS_SRC_DIR}/nvidia/NvCodec/NvCodec"
    "${WEBRTC_INCLUDE}"
    "${WEBRTC_INCLUDE}/third_party/abseil-cpp"
    "${WEBRTC_INCLUDE}/third_party/libyuv/include"
    "${WEBRTC_INCLUDE}/buildtools/third_party/libc++"
    "${CUDA_HOME}/include"
  )
  target_compile_options(${target} PRIVATE
    -nostdinc++
    "-isystem${WEBRTC_LIBCXX}"
    "-isystem${WEBRTC_LIBCXXABI}"
    -Wno-deprecated-declarations
    -Wno-nullability-completeness
  )
  target_link_options(${target} PRIVATE -nostdlib++)
  target_link_libraries(${target} PRIVATE
    "${WEBRTC_ROOT}/lib/libwebrtc.a"
    Threads::Threads
    dl
    m
    rt
  )
endfunction()

set(WEBRTC_SYS_NVIDIA_SOURCES
  "${WEBRTC_SYS_SRC_DIR}/av1_bitstream.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/NvCodec/NvCodec/NvDecoder/NvDecoder.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/NvCodec/NvCodec/NvEncoder/NvEncoder.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/NvCodec/NvCodec/NvEncoder/NvEncoderCuda.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/h264_encoder_impl.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/h265_encoder_impl.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/av1_encoder_impl.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/h264_decoder_impl.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/h265_decoder_impl.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/nvidia_decoder_factory.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/nvidia_encoder_factory.cpp"
  "${WEBRTC_SYS_SRC_DIR}/nvidia/cuda_context.cpp"
  "${WEBRTC_SYS_SRC_DIR}/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libcuda.so.init.c"
  "${WEBRTC_SYS_SRC_DIR}/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libcuda.so.tramp.S"
  "${WEBRTC_SYS_SRC_DIR}/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libnvcuvid.so.init.c"
  "${WEBRTC_SYS_SRC_DIR}/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libnvcuvid.so.tramp.S"
)

include(FetchContent)
FetchContent_Declare(
  googletest
  GIT_REPOSITORY https://github.com/google/googletest.git
  GIT_TAG v1.14.0
  GIT_SHALLOW TRUE
)
set(INSTALL_GTEST OFF CACHE BOOL "" FORCE)
set(BUILD_GMOCK OFF CACHE BOOL "" FORCE)
FetchContent_MakeAvailable(googletest)

# GTest must use the same hermetic libc++ ABI as libwebrtc and the test binary.
foreach(gtest_target gtest gtest_main)
  target_compile_definitions(${gtest_target} PRIVATE ${WEBRTC_COMPILE_DEFINITIONS})
  target_include_directories(${gtest_target} PRIVATE
    "${WEBRTC_INCLUDE}/buildtools/third_party/libc++"
  )
  target_compile_options(${gtest_target} PRIVATE
    -nostdinc++
    "-isystem${WEBRTC_LIBCXX}"
    "-isystem${WEBRTC_LIBCXXABI}"
  )
endforeach()

enable_testing()
include(GoogleTest)

add_subdirectory(benchmark)
add_subdirectory(integration)
