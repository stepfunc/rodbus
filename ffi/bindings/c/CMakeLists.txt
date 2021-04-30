cmake_minimum_required(VERSION 3.8)

project(rodbus_c LANGUAGES C)

# Find rodbus
if(WIN32)
    set(CMAKE_PREFIX_PATH ${CMAKE_CURRENT_LIST_DIR}/generated/x86_64-pc-windows-msvc/cmake)
elseif(UNIX)
    set(CMAKE_PREFIX_PATH ${CMAKE_CURRENT_LIST_DIR}/generated/x86_64-unknown-linux-gnu/cmake)
endif()
find_package(rodbus REQUIRED)

# You probably don't need do this, just pick either the dynamic or the static
# one if you are on Linux. Here we do CMake trickery to make it work on the CI.
if(TARGET rodbus)
    set(rodbus_target rodbus)
else()
    set(rodbus_target rodbus_static)
endif()

# Client example
add_executable(client_example ./client_example.c)
target_link_libraries(client_example PRIVATE ${rodbus_target})

# Server example
add_executable(server_example server_example.c)
target_link_libraries(server_example PRIVATE ${rodbus_target})

if(${rodbus_target} STREQUAL "rodbus")
    # Copy the DLL after build
    add_custom_command(TARGET client_example POST_BUILD 
        COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:client_example>
    )
    add_custom_command(TARGET server_example POST_BUILD 
        COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:server_example>
    )
endif()
