cmake_minimum_required(VERSION 3.15)

project(rodbus-ffi)

set(CARGO_CMD cargo build --release)
set(TARGET_DIR "release")
#set(RODBUS_HEADER_DIR ${CMAKE_CURRENT_BINARY_DIR}/include/rodbus)
#set(RODBUS_LIB "${CMAKE_CURRENT_BINARY_DIR}/${TARGET_DIR}/libclient.so")

#set(RODBUS_HEADERS
#    ${RODBUS_HEADER_DIR}/prelude.h
#    ${RODBUS_HEADER_DIR}/rodbus.h
#    ${RODBUS_HEADER_DIR}/rodbus.hpp
#)

set(LIBRARY_PATH "${CMAKE_CURRENT_BINARY_DIR}/${TARGET_DIR}/librodbus_ffi.a")


# generate the protos
#add_custom_command(
#    OUTPUT ${RODBUS_HEADERS}
#    COMMAND ${CMAKE_COMMAND} -E make_directory ${RODBUS_HEADER_DIR}
#    COMMAND ${CMAKE_COMMAND} -E copy ${CMAKE_CURRENT_SRC_DIR}/prelude.h ${RODBUS_HEADER_DIR}/prelude.h
#    COMMAND cbindgen -c cargobindgen.c.toml -o ${RODBUS_HEADER_DIR}/rodbus.h ..
#    COMMAND cbindgen -c cargobindgen.cpp.toml -o ${RODBUS_HEADER_DIR}/rodbus.hpp ..
#    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
#    COMMENT "Generating Rodbus headers..."
#)

add_custom_target(
    rodbus_ffi ALL
    COMMENT "Compiling rodbus-ffi"
    COMMAND CARGO_TARGET_DIR=${CMAKE_CURRENT_BINARY_DIR} ${CARGO_CMD} 
    COMMAND cp ${LIBRARY_PATH} ${CMAKE_CURRENT_BINARY_DIR}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
)

set_target_properties(rodbus_ffi PROPERTIES LOCATION ${CMAKE_CURRENT_BINARY_DIR})