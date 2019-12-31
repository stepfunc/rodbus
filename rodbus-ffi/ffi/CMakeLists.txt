set(RODBUS_HEADER_DIR ${CMAKE_CURRENT_BINARY_DIR}/include/rodbus)

set(RODBUS_HEADERS
    ${RODBUS_HEADER_DIR}/prelude.h
    ${RODBUS_HEADER_DIR}/rodbus.h
    ${RODBUS_HEADER_DIR}/rodbus.hpp
)



# generate the protos
add_custom_command(
    OUTPUT ${RODBUS_HEADER_DIR}/rodbus.h ${RODBUS_HEADER_DIR}/rodbus.hpp
    COMMAND ${CMAKE_COMMAND} -E make_directory ${RODBUS_HEADER_DIR}
    COMMAND cbindgen -c cargobindgen.c.toml -o ${RODBUS_HEADER_DIR}/rodbus.h ..
    COMMAND cbindgen -c cargobindgen.cpp.toml -o ${RODBUS_HEADER_DIR}/rodbus.hpp ..
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMENT "Generating Rodbus headers..."
)

add_library(rodbus_ffi ${RODBUS_HEADERS})