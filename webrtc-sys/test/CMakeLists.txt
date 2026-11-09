# Project-level configuration.
cmake_minimum_required(VERSION 3.10)
project(runner LANGUAGES CXX C)
enable_language(ASM)

set(BINARY_NAME "h264_benchmark")

# Set the C++ standard to C++23.
set(CMAKE_CXX_STANDARD 20)

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

# Keep benchmark compilation ABI-compatible with the pinned libwebrtc archive.
foreach(ninja_file webrtc.ninja desktop_capture.ninja)
  file(STRINGS "${WEBRTC_ROOT}/${ninja_file}" ninja_defines_line LIMIT_COUNT 1)
  string(REGEX MATCHALL "-D[A-Za-z_][A-Za-z0-9_]*(=[^ ]+)?" ninja_defines
    "${ninja_defines_line}")
  foreach(ninja_define IN LISTS ninja_defines)
    if(NOT ninja_define MATCHES "^-D(CR_CLANG_REVISION|CR_XCODE_VERSION)=")
      add_definitions("${ninja_define}")
    endif()
  endforeach()
endforeach()

add_definitions(-DUSE_NVIDIA_VIDEO_CODEC=1)
add_definitions(-DUSE_VAAPI_VIDEO_CODEC=1)

include_directories(
  "${WEBRTC_INCLUDE}"
  "${WEBRTC_INCLUDE}/third_party/abseil-cpp"
  "${WEBRTC_INCLUDE}/third_party/libyuv/include"
  "${WEBRTC_INCLUDE}/buildtools/third_party/libc++"
  "${CMAKE_CURRENT_SOURCE_DIR}/../src/nvidia/NvCodec/include"
  "${CMAKE_CURRENT_SOURCE_DIR}/../src/nvidia/NvCodec/NvCodec"
  "${CMAKE_CURRENT_SOURCE_DIR}/../src"
  "${CUDA_HOME}/include"
)

link_libraries(
  "${WEBRTC_ROOT}/lib/libwebrtc.a"
)

find_package (Threads)

add_executable(${BINARY_NAME}
  "test_main.cc"
  "benchmark.cc"
  "benchmark_openh264.cc"
  "video_source.cc"
  "fileutils.cc"
  "cpu/cpu_linux.cc"

  "benchmark_nvidia.cc"
  "../src/nvidia/NvCodec/NvCodec/NvDecoder/NvDecoder.cpp"
  "../src/nvidia/NvCodec/NvCodec/NvEncoder/NvEncoder.cpp"
  "../src/nvidia/NvCodec/NvCodec/NvEncoder/NvEncoderCuda.cpp"
  "../src/av1_bitstream.cpp"
  "../src/nvidia/h264_encoder_impl.cpp"
  "../src/nvidia/h265_encoder_impl.cpp"
  "../src/nvidia/av1_encoder_impl.cpp"
  "../src/nvidia/h264_decoder_impl.cpp"
  "../src/nvidia/h265_decoder_impl.cpp"
  "../src/nvidia/nvidia_decoder_factory.cpp"
  "../src/nvidia/nvidia_encoder_factory.cpp"
  "../src/nvidia/cuda_context.cpp"
  "../src/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libcuda.so.init.c"
  "../src/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libcuda.so.tramp.S"
  "../src/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libnvcuvid.so.init.c"
  "../src/lazy_load_deps_for/nvidia/x86_64-linux-gnu/libnvcuvid.so.tramp.S"

  "benchmark_vaapi.cc"
  "../src/vaapi/vaapi_display_drm.cpp"
  "../src/vaapi/vaapi_h264_encoder_wrapper.cpp"
  "../src/vaapi/vaapi_encoder_factory.cpp"
  "../src/vaapi/h264_encoder_impl.cpp"
  "../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva-drm.so.init.c"
  "../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva-drm.so.tramp.S"
  "../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva.so.init.c"
  "../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva.so.tramp.S"
)

target_compile_options(${BINARY_NAME} PRIVATE
  -nostdinc++
  "-isystem${WEBRTC_LIBCXX}"
  "-isystem${WEBRTC_LIBCXXABI}"
  -Wno-deprecated-declarations
  -Wno-nullability-completeness
)
target_link_options(${BINARY_NAME} PRIVATE -nostdlib++)
target_link_libraries(${BINARY_NAME} ${CMAKE_THREAD_LIBS_INIT})
target_link_libraries(${BINARY_NAME} dl m rt)
