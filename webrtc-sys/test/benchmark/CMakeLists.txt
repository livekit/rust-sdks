set(BINARY_NAME "h264_benchmark")

add_executable(${BINARY_NAME}
  "test_main.cc"
  "benchmark.cc"
  "benchmark_openh264.cc"
  "video_source.cc"
  "fileutils.cc"
  "cpu/cpu_linux.cc"

  "benchmark_nvidia.cc"
  ${WEBRTC_SYS_NVIDIA_SOURCES}

  "benchmark_vaapi.cc"
  "../../src/vaapi/vaapi_display_drm.cpp"
  "../../src/vaapi/vaapi_h264_encoder_wrapper.cpp"
  "../../src/vaapi/vaapi_encoder_factory.cpp"
  "../../src/vaapi/h264_encoder_impl.cpp"
  "../../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva-drm.so.init.c"
  "../../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva-drm.so.tramp.S"
  "../../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva.so.init.c"
  "../../src/lazy_load_deps_for/vaapi/x86_64-linux-gnu/libva.so.tramp.S"
)

target_compile_definitions(${BINARY_NAME} PRIVATE
  USE_VAAPI_VIDEO_CODEC=1
)
configure_webrtc_test_target(${BINARY_NAME})
