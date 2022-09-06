cmake_minimum_required(VERSION 3.12)

project(rodbus_c LANGUAGES C CXX)

set(RODBUS_VERSION "1.0.0")

# Determine the architecture
if(WIN32 AND CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|amd64|AMD64" AND CMAKE_SIZEOF_VOID_P EQUAL 8)
    set(RODBUS_ARCH "x86_64-pc-windows-msvc")
elseif(UNIX AND CMAKE_SYSTEM_PROCESSOR MATCHES "x86_64|amd64|AMD64")
    set(RODBUS_ARCH "x86_64-unknown-linux-gnu")
elseif(UNIX AND CMAKE_SYSTEM_PROCESSOR MATCHES "aarch64")
    set(RODBUS_ARCH "aarch64-unknown-linux-gnu")
else()
    message(FATAL_ERROR "target architecture not supported by this CMake file")
endif()

# Find the Rodbus package
if(EXISTS ${CMAKE_CURRENT_LIST_DIR}/generated/${RODBUS_ARCH})
    # Use the locally built library
    find_package(rodbus REQUIRED HINTS ${CMAKE_CURRENT_LIST_DIR}/generated/${RODBUS_ARCH}/cmake)
else()
    # Download the library from GitHub
    include(FetchContent)
    FetchContent_Declare(
        rodbus
        URL https://github.com/stepfunc/rodbus/releases/download/${RODBUS_VERSION}/rodbus-${RODBUS_VERSION}-${RODBUS_ARCH}.zip
    )

    FetchContent_GetProperties(rodbus)
    if(NOT rodbus_POPULATED)
        FetchContent_Populate(rodbus)
        find_package(rodbus REQUIRED HINTS ${rodbus_SOURCE_DIR})
    endif()
endif()

# -------------- C examples -----------------

# C Client example
add_executable(client_example ./client_example.c)
target_link_libraries(client_example PRIVATE rodbus)

# C Server example
add_executable(server_example server_example.c)
target_link_libraries(server_example PRIVATE rodbus)

# -------------- C++ examples -----------------

# C++ Master example
add_executable(cpp_client_example client_example.cpp)
target_link_libraries(cpp_client_example PRIVATE rodbus_cpp)

# C++ Outstation example
add_executable(cpp_server_example server_example.cpp )
target_link_libraries(cpp_server_example PRIVATE rodbus_cpp)

# Copy the DLL after build
add_custom_command(TARGET client_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:client_example>
)
add_custom_command(TARGET server_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:server_example>
)
add_custom_command(TARGET cpp_client_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:cpp_client_example>
)
add_custom_command(TARGET cpp_server_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:cpp_server_example>
)
