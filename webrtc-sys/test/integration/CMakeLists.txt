add_executable(webrtc_sys_integration_tests
  nvidia_hardware_test.cc
  ${WEBRTC_SYS_NVIDIA_SOURCES}
)

target_link_libraries(webrtc_sys_integration_tests PRIVATE
  GTest::gtest_main
  GTest::gtest
)
configure_webrtc_test_target(webrtc_sys_integration_tests)

add_test(
  NAME webrtc_sys_nvidia_integration
  COMMAND webrtc_sys_integration_tests
)
set_tests_properties(webrtc_sys_nvidia_integration PROPERTIES
  LABELS "integration;gpu;nvidia"
)
