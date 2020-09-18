cmake_minimum_required (VERSION 3.11)

project(rodbus_c_demo VERSION 1.0.0)

include("generated/cmake/rodbus-config.cmake")

add_executable(rodbus-client ./src/client.c)
target_link_libraries (rodbus-client PRIVATE rodbus)

add_executable(rodbus-server ./src/server.c)
target_link_libraries (rodbus-server PRIVATE rodbus)
