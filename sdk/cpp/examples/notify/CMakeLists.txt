add_executable(whether-notify main.cpp)
target_link_libraries(whether-notify PRIVATE whether::plugin)
install(TARGETS whether-notify)

if(WHETHER_SDK_TESTS)
  add_executable(test_notify tests/test_notify.cpp)
  target_link_libraries(test_notify PRIVATE whether::plugin)
  target_compile_definitions(test_notify PRIVATE
    WHETHER_EXAMPLES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../../../schema/examples"
    FIXTURES_DIR="${CMAKE_CURRENT_SOURCE_DIR}/tests")
  add_test(NAME notify COMMAND test_notify)
endif()
