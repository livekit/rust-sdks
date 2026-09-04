cmake_minimum_required(VERSION 3.19)
project(livekit_uniffi_cpp_smoke LANGUAGES CXX)

if(NOT DEFINED UNIFFI_BINDGEN_CPP)
    message(FATAL_ERROR "Set UNIFFI_BINDGEN_CPP to the uniffi-bindgen-cpp executable")
endif()
if(NOT DEFINED LIVEKIT_UNIFFI_LIBRARY)
    message(FATAL_ERROR "Set LIVEKIT_UNIFFI_LIBRARY to the host livekit-uniffi dynamic library")
endif()

set(GENERATED_DIR "${CMAKE_CURRENT_BINARY_DIR}/generated")
file(MAKE_DIRECTORY "${GENERATED_DIR}")
execute_process(
    COMMAND "${UNIFFI_BINDGEN_CPP}" --library "${LIVEKIT_UNIFFI_LIBRARY}" --out-dir "${GENERATED_DIR}"
    COMMAND_ERROR_IS_FATAL ANY
)

set(GENERATED_SOURCES
    "${GENERATED_DIR}/livekit_common.cpp"
    "${GENERATED_DIR}/livekit_datatrack.cpp"
    "${GENERATED_DIR}/livekit_net.cpp"
    "${GENERATED_DIR}/livekit_uniffi.cpp"
)

find_package(Threads REQUIRED)
add_executable(livekit-uniffi-cpp-smoke main.cpp ${GENERATED_SOURCES})
target_compile_features(livekit-uniffi-cpp-smoke PRIVATE cxx_std_17)
set_target_properties(livekit-uniffi-cpp-smoke PROPERTIES CXX_EXTENSIONS OFF)
target_include_directories(livekit-uniffi-cpp-smoke PRIVATE "${GENERATED_DIR}")
target_link_libraries(livekit-uniffi-cpp-smoke PRIVATE "${LIVEKIT_UNIFFI_LIBRARY}" Threads::Threads)

if(MSVC)
    target_compile_options(livekit-uniffi-cpp-smoke PRIVATE /permissive-)
else()
    target_compile_options(livekit-uniffi-cpp-smoke PRIVATE -Wall -Wextra -Wpedantic -pedantic-errors)
endif()

enable_testing()
add_test(NAME livekit-uniffi-cpp-smoke COMMAND livekit-uniffi-cpp-smoke)
