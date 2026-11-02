cmake_minimum_required(VERSION 3.28)
project(whether_plugin_sdk LANGUAGES CXX)

include(FetchContent)
# nlohmann/json 3.12.0, the release tarball pinned by its published SHA-256. An
# installed nlohmann_json 3.12 or newer is used instead when CMake finds one.
FetchContent_Declare(nlohmann_json
  URL https://github.com/nlohmann/json/releases/download/v3.12.0/json.tar.xz
  URL_HASH SHA256=42f6e95cad6ec532fd372391373363b62a14af6d771056dbfc86160e6dfff7aa
  DOWNLOAD_EXTRACT_TIMESTAMP TRUE
  SYSTEM
  FIND_PACKAGE_ARGS 3.12)
FetchContent_MakeAvailable(nlohmann_json)

add_library(whether_plugin INTERFACE)
add_library(whether::plugin ALIAS whether_plugin)
target_include_directories(whether_plugin INTERFACE
  $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>)
target_compile_features(whether_plugin INTERFACE cxx_std_23)
target_link_libraries(whether_plugin INTERFACE nlohmann_json::nlohmann_json)

option(WHETHER_SDK_TESTS "Build the SDK tests" ${PROJECT_IS_TOP_LEVEL})
if(WHETHER_SDK_TESTS)
  enable_testing()
  add_executable(test_protocol tests/test_protocol.cpp)
  target_link_libraries(test_protocol PRIVATE whether::plugin)
  # The golden examples written by `cargo xtask schema`, shared with the Rust tests.
  target_compile_definitions(test_protocol PRIVATE
    WHETHER_EXAMPLES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../schema/examples")
  add_test(NAME protocol COMMAND test_protocol)
endif()

option(WHETHER_SDK_EXAMPLES "Build the example plugins" ${PROJECT_IS_TOP_LEVEL})
if(WHETHER_SDK_EXAMPLES)
  add_subdirectory(examples/notify)
endif()
