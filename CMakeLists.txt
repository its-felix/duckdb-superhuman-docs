cmake_minimum_required(VERSION 3.5...3.29)

set(TARGET_NAME superhuman_docs)
set(EXTENSION_NAME ${TARGET_NAME}_extension)
set(RUST_EXTENSION_CRATE_NAME "superhuman_docs_rust")

project(${TARGET_NAME})

include_directories(src/include)
find_program(CARGO_EXECUTABLE cargo REQUIRED)

option(SUPERHUMAN_DOCS_BUILTIN_ONLY
       "Build only the static extension target for embedding in DuckDB-Wasm"
       OFF)

set(RUST_EXTENSION_DUCKDB_PLATFORM "${DUCKDB_EXPLICIT_PLATFORM}")
if(DEFINED ENV{DUCKDB_PLATFORM} AND NOT "$ENV{DUCKDB_PLATFORM}" STREQUAL "")
  set(RUST_EXTENSION_DUCKDB_PLATFORM "$ENV{DUCKDB_PLATFORM}")
endif()

string(REPLACE "_" ";" RUST_BRIDGE_EXTENSION_NAME_PARTS ${TARGET_NAME})
set(RUST_BRIDGE_EXTENSION_CLASS "")
foreach(NAME_PART IN LISTS RUST_BRIDGE_EXTENSION_NAME_PARTS)
  string(SUBSTRING ${NAME_PART} 0 1 FIRST_LETTER)
  string(SUBSTRING ${NAME_PART} 1 -1 REMAINDER)
  string(TOUPPER ${FIRST_LETTER} FIRST_LETTER)
  set(RUST_BRIDGE_EXTENSION_CLASS
      "${RUST_BRIDGE_EXTENSION_CLASS}${FIRST_LETTER}${REMAINDER}")
endforeach()
set(RUST_BRIDGE_EXTENSION_CLASS "${RUST_BRIDGE_EXTENSION_CLASS}Extension")

set(RUST_EXTENSION_TARGET_DIR "${CMAKE_CURRENT_BINARY_DIR}/cargo-target")
set(RUST_EXTENSION_LIB_NAME "lib${RUST_EXTENSION_CRATE_NAME}.a")
set(RUST_EXTENSION_CARGO_TARGET "")
if(RUST_EXTENSION_DUCKDB_PLATFORM STREQUAL "osx_amd64")
  set(RUST_EXTENSION_CARGO_TARGET "x86_64-apple-darwin")
elseif(RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^wasm_")
  set(RUST_EXTENSION_CARGO_TARGET "wasm32-unknown-emscripten")
elseif(RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^windows_amd64_(rtools|mingw)$")
  set(RUST_EXTENSION_CARGO_TARGET "x86_64-pc-windows-gnu")
endif()

if(WIN32 OR RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^windows_")
  set(RUST_EXTENSION_LIB_NAME "${RUST_EXTENSION_CRATE_NAME}.lib")
endif()
if(RUST_EXTENSION_CARGO_TARGET MATCHES "windows-gnu")
  set(RUST_EXTENSION_LIB_NAME "lib${RUST_EXTENSION_CRATE_NAME}.a")
endif()

set(RUST_EXTENSION_LIB_DIR "${RUST_EXTENSION_TARGET_DIR}/release")
set(RUST_EXTENSION_CARGO_BUILD_ARGS build --release)
if(NOT RUST_EXTENSION_CARGO_TARGET STREQUAL "")
  list(APPEND RUST_EXTENSION_CARGO_BUILD_ARGS
       "--target=${RUST_EXTENSION_CARGO_TARGET}")
  set(RUST_EXTENSION_LIB_DIR
      "${RUST_EXTENSION_TARGET_DIR}/${RUST_EXTENSION_CARGO_TARGET}/release")
endif()
list(APPEND RUST_EXTENSION_CARGO_BUILD_ARGS --manifest-path
     "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")
set(RUST_EXTENSION_LIB
    "${RUST_EXTENSION_LIB_DIR}/${RUST_EXTENSION_LIB_NAME}")
set(RUST_EXTENSION_CARGO_ENV
    "CARGO_TARGET_DIR=${RUST_EXTENSION_TARGET_DIR}")
if(RUST_EXTENSION_DUCKDB_PLATFORM STREQUAL "wasm_threads")
  # Rust's distributed Emscripten std is single-threaded. Rebuild it with the
  # same Wasm atomics features as DuckDB's threaded bundle.
  list(APPEND RUST_EXTENSION_CARGO_BUILD_ARGS
       -Z build-std=std,panic_abort)
  list(APPEND RUST_EXTENSION_CARGO_ENV
       "RUSTC_BOOTSTRAP=1"
       "RUSTFLAGS=-C target-feature=+atomics,+bulk-memory,+mutable-globals")
endif()
file(GLOB_RECURSE RUST_EXTENSION_SOURCES CONFIGURE_DEPENDS
     "${CMAKE_CURRENT_SOURCE_DIR}/src/*.rs")

add_custom_command(
  OUTPUT "${RUST_EXTENSION_LIB}"
  COMMAND "${CMAKE_COMMAND}" -E env ${RUST_EXTENSION_CARGO_ENV}
          "${CARGO_EXECUTABLE}" ${RUST_EXTENSION_CARGO_BUILD_ARGS}
  WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
  DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml"
          "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.lock" ${RUST_EXTENSION_SOURCES}
  COMMENT "Building Rust extension core")
add_custom_target(rust_extension_build DEPENDS "${RUST_EXTENSION_LIB}")

set(EXTENSION_SOURCES
    src/cpp/rust_bridge_client.cpp
    src/cpp/rust_bridge_client_dml.cpp
    src/cpp/rust_bridge_extension.cpp
    src/cpp/rust_bridge_scan.cpp
    src/cpp/rust_bridge_scan_planning.cpp
    src/cpp/rust_bridge_secret.cpp
    src/cpp/rust_bridge_storage.cpp
    src/cpp/rust_bridge_value.cpp
    src/cpp/storage/rust_bridge_catalog.cpp
    src/cpp/storage/rust_bridge_catalog_planning.cpp
    src/cpp/storage/rust_bridge_dml.cpp
    src/cpp/storage/rust_bridge_schema.cpp
    src/cpp/storage/rust_bridge_table.cpp
    src/cpp/storage/rust_bridge_transaction_manager.cpp)

if(RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^wasm_")
  list(APPEND EXTENSION_SOURCES src/cpp/emscripten_fetch.cpp)
endif()

add_library(${EXTENSION_NAME} STATIC ${EXTENSION_SOURCES})
if(NOT SUPERHUMAN_DOCS_BUILTIN_ONLY)
  build_loadable_extension(${TARGET_NAME} "-warnings" ${EXTENSION_SOURCES})
endif()

add_dependencies(${EXTENSION_NAME} rust_extension_build)
target_compile_definitions(
  ${EXTENSION_NAME} PRIVATE RUST_BRIDGE_EXTENSION_ENTRY=${TARGET_NAME}
                            RUST_BRIDGE_EXTENSION_CLASS=${RUST_BRIDGE_EXTENSION_CLASS})
if(NOT SUPERHUMAN_DOCS_BUILTIN_ONLY)
  add_dependencies(${TARGET_NAME}_loadable_extension rust_extension_build)
  target_compile_definitions(
    ${TARGET_NAME}_loadable_extension
    PRIVATE RUST_BRIDGE_EXTENSION_ENTRY=${TARGET_NAME}
            RUST_BRIDGE_EXTENSION_CLASS=${RUST_BRIDGE_EXTENSION_CLASS})
endif()

set(RUST_EXTENSION_SYSTEM_LIBS "")
if(APPLE OR RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^osx_")
  find_library(RUST_EXTENSION_SECURITY_FRAMEWORK Security REQUIRED)
  find_library(RUST_EXTENSION_CORE_FOUNDATION_FRAMEWORK CoreFoundation REQUIRED)
  list(APPEND RUST_EXTENSION_SYSTEM_LIBS
       "${RUST_EXTENSION_SECURITY_FRAMEWORK}"
       "${RUST_EXTENSION_CORE_FOUNDATION_FRAMEWORK}")
endif()
if(WIN32 OR RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^windows_")
  list(APPEND RUST_EXTENSION_SYSTEM_LIBS ntdll userenv ws2_32 bcrypt advapi32
       crypt32 secur32)
endif()

target_link_libraries(${EXTENSION_NAME} "${RUST_EXTENSION_LIB}"
                      ${RUST_EXTENSION_SYSTEM_LIBS})
if(NOT SUPERHUMAN_DOCS_BUILTIN_ONLY)
  target_link_libraries(${TARGET_NAME}_loadable_extension
                        "${RUST_EXTENSION_LIB}" ${RUST_EXTENSION_SYSTEM_LIBS})
endif()

if(RUST_EXTENSION_DUCKDB_PLATFORM MATCHES "^wasm_")
  set(RUST_EXTENSION_LINK_TARGETS ${EXTENSION_NAME})
  if(NOT SUPERHUMAN_DOCS_BUILTIN_ONLY)
    list(APPEND RUST_EXTENSION_LINK_TARGETS ${TARGET_NAME}_loadable_extension)
  endif()
  foreach(RUST_EXTENSION_LINK_TARGET IN LISTS RUST_EXTENSION_LINK_TARGETS)
    target_link_options(
      ${RUST_EXTENSION_LINK_TARGET}
      PUBLIC "SHELL:-sFETCH=1" "SHELL:-sASYNCIFY=1"
             "SHELL:-sASYNCIFY_STACK_SIZE=262144")
  endforeach()
endif()

install(
  TARGETS ${EXTENSION_NAME}
  EXPORT "${DUCKDB_EXPORT_SET}"
  LIBRARY DESTINATION "${INSTALL_LIB_DIR}"
  ARCHIVE DESTINATION "${INSTALL_LIB_DIR}")
install(FILES "${RUST_EXTENSION_LIB}" DESTINATION "${INSTALL_LIB_DIR}")
