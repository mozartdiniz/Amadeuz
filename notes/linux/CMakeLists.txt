cmake_minimum_required(VERSION 3.20)
project(amadeuz CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(PkgConfig REQUIRED)
pkg_check_modules(GTK4      REQUIRED IMPORTED_TARGET gtk4)
pkg_check_modules(SOUP3     REQUIRED IMPORTED_TARGET libsoup-3.0)
pkg_check_modules(JSON_GLIB REQUIRED IMPORTED_TARGET json-glib-1.0)

add_executable(amadeuz
    src/main.cpp
    src/local_store.cpp
    src/sync_service.cpp
    src/note_view_model.cpp
    src/main_window.cpp
)

target_link_libraries(amadeuz PRIVATE
    PkgConfig::GTK4
    PkgConfig::SOUP3
    PkgConfig::JSON_GLIB
)

target_include_directories(amadeuz PRIVATE src)
