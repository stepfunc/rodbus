cmake_minimum_required(VERSION 3.10)

project(rodbus-ffi C)

if(NOT UNIX)
    message(FATAL_ERROR "examples only compile on *NIX ATM")
endif()

set(CARGO_CMD cargo build --release)
set(TARGET_DIR "target/release")
set(INCLUDES_DIR ${CMAKE_CURRENT_BINARY_DIR}/include)
set(RODBUS_HEADER_DIR ${INCLUDES_DIR}/rodbus)

set(LIBRARY_NAME librodbus_ffi.so)
get_filename_component(CRATE_BASE_DIR ../../ ABSOLUTE)
set(LIBRARY_PATH "${CRATE_BASE_DIR}/${TARGET_DIR}/${LIBRARY_NAME}")


add_custom_target(cbindgen
        COMMENT "running cbindgen"
        COMMAND ${CMAKE_COMMAND} -E make_directory ${RODBUS_HEADER_DIR}
        COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SOURCE_DIR}/prelude.h ${RODBUS_HEADER_DIR}/prelude.h
        COMMAND cbindgen -c cbindgen.c.toml -o ${RODBUS_HEADER_DIR}/rodbus.h ..
        COMMAND cbindgen -c cbindgen.cpp.toml -o ${RODBUS_HEADER_DIR}/rodbus.hpp ..
        WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(build
    COMMENT "Compiling rodbus-ffi"
    COMMAND cargo build --release
    WORKING_DIRECTORY ${CRATE_BASE_DIR}
)

add_custom_target(doc
    COMMENT "Building docs"
    COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SOURCE_DIR}/main.md ${CMAKE_CURRENT_BINARY_DIR}/main.md
    COMMAND doxygen ${CMAKE_CURRENT_SOURCE_DIR}/doxygen.config
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
    DEPENDS cbindgen
)


install(FILES ${LIBRARY_PATH} DESTINATION lib)
install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/include/rodbus DESTINATION include)

add_executable(sync_client ${CMAKE_CURRENT_SOURCE_DIR}/examples/sync_client.c)
target_link_libraries(sync_client ${LIBRARY_PATH})
target_include_directories(sync_client PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/include")
add_dependencies(sync_client cbindgen build)

add_executable(async_client ${CMAKE_CURRENT_SOURCE_DIR}/examples/async_client.c)
target_link_libraries(async_client ${LIBRARY_PATH})
target_include_directories(async_client PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/include")
add_dependencies(async_client cbindgen build)

add_executable(server ${CMAKE_CURRENT_SOURCE_DIR}/examples/server.c)
target_link_libraries(server ${LIBRARY_PATH})
target_include_directories(server PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/include")
add_dependencies(server cbindgen build)
