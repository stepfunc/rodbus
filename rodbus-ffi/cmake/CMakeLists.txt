cmake_minimum_required(VERSION 3.10)

project(rodbus-ffi NONE)

set(CARGO_CMD cargo build --release)
set(TARGET_DIR "release")
set(RODBUS_HEADER_DIR ${CMAKE_CURRENT_BINARY_DIR}/include/rodbus)

set(LIBRARY_NAME librodbus_ffi.so)
set(LIBRARY_PATH "${CMAKE_CURRENT_BINARY_DIR}/${TARGET_DIR}/${LIBRARY_NAME}")

add_custom_target(build
    COMMENT "Compiling rodbus-ffi"
    COMMAND ${CMAKE_COMMAND} -E make_directory ${RODBUS_HEADER_DIR}
    COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SOURCE_DIR}/prelude.h ${RODBUS_HEADER_DIR}/prelude.h
    COMMAND cbindgen -c cbindgen.c.toml -o ${RODBUS_HEADER_DIR}/rodbus.h ..
    COMMAND cbindgen -c cbindgen.cpp.toml -o ${RODBUS_HEADER_DIR}/rodbus.hpp ..
    COMMAND CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} 
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

add_custom_target(doc
    COMMENT "Building docs"
    COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SOURCE_DIR}/main.md ${CMAKE_CURRENT_BINARY_DIR}/main.md
    COMMAND doxygen ${CMAKE_CURRENT_SOURCE_DIR}/doxygen.config
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
    DEPENDS build
)


install(FILES ${LIBRARY_PATH} DESTINATION lib)
install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/include/rodbus DESTINATION include)
