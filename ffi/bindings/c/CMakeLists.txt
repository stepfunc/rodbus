cmake_minimum_required(VERSION 3.8)

project(rodbus_c LANGUAGES C)

# Find rodbus
if(WIN32)
    set(CMAKE_PREFIX_PATH ${CMAKE_CURRENT_LIST_DIR}/generated/x86_64-pc-windows-msvc/cmake)
elseif(UNIX)
    set(CMAKE_PREFIX_PATH ${CMAKE_CURRENT_LIST_DIR}/generated/x86_64-unknown-linux-gnu/cmake)
endif()
find_package(rodbus REQUIRED)

# Client example
add_executable(client_example ./client_example.c)
target_link_libraries(client_example PRIVATE rodbus)

# Server example
add_executable(server_example server_example.c)
target_link_libraries(server_example PRIVATE rodbus)

# Copy the DLL after build
add_custom_command(TARGET client_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:client_example>
)
add_custom_command(TARGET server_example POST_BUILD 
    COMMAND ${CMAKE_COMMAND} -E copy_if_different $<TARGET_FILE:rodbus> $<TARGET_FILE_DIR:server_example>
)
