# ============================================================================
# RAX C/C++ API — CMake build & packaging
#
# Builds the `librax` native library (via cargo) from the `rax-capi` crate and
# exposes it as imported CMake targets:
#
#   rax::rax          shared library (librax.so / librax.dylib)
#   rax::rax_static   static  library (librax.a) + required system link libs
#
# Downstream use after `find_package(rax)`:
#
#   target_link_libraries(myapp PRIVATE rax::rax)          # dynamic
#   target_link_libraries(myapp PRIVATE rax::rax_static)   # static
#
# Build/install:
#   cmake -S capi -B build -DCMAKE_INSTALL_PREFIX=/usr/local
#   cmake --build build
#   cmake --install build
# ============================================================================
cmake_minimum_required(VERSION 3.15)

project(rax
        VERSION 1.1.0
        DESCRIPTION "Stable C/C++ API for the RAX emulation engine"
        LANGUAGES C)

include(GNUInstallDirs)
include(CMakePackageConfigHelpers)

# ----------------------------------------------------------------------------
# Options
# ----------------------------------------------------------------------------
# Cargo profile: "release" (default) or "dev". The cargo *profile* name maps to
# an output *directory* name ("dev" -> "debug").
set(RAX_CARGO_PROFILE "release" CACHE STRING "cargo profile: release|dev")
option(RAX_BUILD_SHARED "Build/expose the shared library target" ON)
option(RAX_BUILD_STATIC "Build/expose the static library target" ON)
# Optional cargo feature list, e.g. -DRAX_FEATURES="jit;hvf"
set(RAX_FEATURES "" CACHE STRING "extra cargo features (semicolon-separated)")

if(RAX_CARGO_PROFILE STREQUAL "release")
    set(_rax_profile_dir "release")
    set(_rax_cargo_profile_flag "--release")
elseif(RAX_CARGO_PROFILE STREQUAL "dev" OR RAX_CARGO_PROFILE STREQUAL "debug")
    set(_rax_profile_dir "debug")
    set(_rax_cargo_profile_flag "")
else()
    # Custom named profile: cargo places output under target/<profile>/.
    set(_rax_profile_dir "${RAX_CARGO_PROFILE}")
    set(_rax_cargo_profile_flag "--profile" "${RAX_CARGO_PROFILE}")
endif()

# ----------------------------------------------------------------------------
# Locate cargo
# ----------------------------------------------------------------------------
find_program(CARGO_EXECUTABLE cargo
             HINTS "$ENV{HOME}/.cargo/bin" "$ENV{CARGO_HOME}/bin")
if(NOT CARGO_EXECUTABLE)
    message(FATAL_ERROR "cargo not found; install the Rust toolchain (https://rustup.rs)")
endif()

set(_rax_manifest "${CMAKE_CURRENT_SOURCE_DIR}/Cargo.toml")
# Force a deterministic output location inside the build tree.
set(_rax_target_dir "${CMAKE_CURRENT_BINARY_DIR}/cargo")
set(_rax_out_dir "${_rax_target_dir}/${_rax_profile_dir}")

# Platform-specific library file names.
if(APPLE)
    set(_rax_shared_name "librax.dylib")
elseif(WIN32)
    set(_rax_shared_name "rax.dll")
else()
    set(_rax_shared_name "librax.so")
endif()
set(_rax_static_name "${CMAKE_STATIC_LIBRARY_PREFIX}rax${CMAKE_STATIC_LIBRARY_SUFFIX}")

set(_rax_shared_path "${_rax_out_dir}/${_rax_shared_name}")
set(_rax_static_path "${_rax_out_dir}/${_rax_static_name}")

# Assemble cargo feature flags.
set(_rax_feature_flags "")
if(RAX_FEATURES)
    string(REPLACE ";" "," _rax_feature_csv "${RAX_FEATURES}")
    set(_rax_feature_flags "--features" "${_rax_feature_csv}")
endif()

# ----------------------------------------------------------------------------
# Build the library with cargo
# ----------------------------------------------------------------------------
set(_rax_outputs "")
if(RAX_BUILD_SHARED)
    list(APPEND _rax_outputs "${_rax_shared_path}")
endif()
if(RAX_BUILD_STATIC)
    list(APPEND _rax_outputs "${_rax_static_path}")
endif()

add_custom_command(
    OUTPUT ${_rax_outputs}
    COMMAND ${CMAKE_COMMAND} -E env "CARGO_TARGET_DIR=${_rax_target_dir}"
            ${CARGO_EXECUTABLE} build --package rax-capi
            --manifest-path "${_rax_manifest}"
            ${_rax_cargo_profile_flag} ${_rax_feature_flags}
    WORKING_DIRECTORY "${CMAKE_CURRENT_SOURCE_DIR}"
    COMMENT "Building librax via cargo (${RAX_CARGO_PROFILE})"
    VERBATIM)

add_custom_target(rax_cargo ALL DEPENDS ${_rax_outputs})

# ----------------------------------------------------------------------------
# Platform system libraries required when linking the *static* Rust library.
# (The shared library already embeds these.)
# ----------------------------------------------------------------------------
set(_rax_static_syslibs "")
if(APPLE)
    set(_rax_static_syslibs
        "-framework CoreFoundation"
        "-framework Security"
        "-framework SystemConfiguration"
        "-liconv"
        "-lobjc"
        Threads::Threads)
elseif(WIN32)
    set(_rax_static_syslibs ws2_32 userenv ntdll bcrypt)
else() # Linux / other Unix
    set(_rax_static_syslibs Threads::Threads ${CMAKE_DL_LIBS} m rt util)
endif()

if(NOT WIN32)
    set(THREADS_PREFER_PTHREAD_FLAG ON)
    find_package(Threads REQUIRED)
endif()

# ----------------------------------------------------------------------------
# Imported targets
# ----------------------------------------------------------------------------
if(RAX_BUILD_SHARED)
    add_library(rax_shared SHARED IMPORTED GLOBAL)
    set_target_properties(rax_shared PROPERTIES
        IMPORTED_LOCATION "${_rax_shared_path}"
        INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_CURRENT_SOURCE_DIR}/include")
    add_dependencies(rax_shared rax_cargo)
    add_library(rax::rax ALIAS rax_shared)
endif()

if(RAX_BUILD_STATIC)
    add_library(rax_static STATIC IMPORTED GLOBAL)
    set_target_properties(rax_static PROPERTIES
        IMPORTED_LOCATION "${_rax_static_path}"
        INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_CURRENT_SOURCE_DIR}/include"
        INTERFACE_LINK_LIBRARIES "${_rax_static_syslibs}")
    add_dependencies(rax_static rax_cargo)
    add_library(rax::rax_static ALIAS rax_static)
endif()

# ----------------------------------------------------------------------------
# Install: headers, libraries, pkg-config, CMake package config
# ----------------------------------------------------------------------------
install(FILES
    "${CMAKE_CURRENT_SOURCE_DIR}/include/rax.h"
    "${CMAKE_CURRENT_SOURCE_DIR}/include/rax.hpp"
    DESTINATION "${CMAKE_INSTALL_INCLUDEDIR}"
    OPTIONAL) # rax.hpp may not be present in minimal checkouts

if(RAX_BUILD_SHARED)
    install(FILES "${_rax_shared_path}" DESTINATION "${CMAKE_INSTALL_LIBDIR}")
endif()
if(RAX_BUILD_STATIC)
    install(FILES "${_rax_static_path}" DESTINATION "${CMAKE_INSTALL_LIBDIR}")
endif()

# pkg-config file.
set(PKG_PREFIX "${CMAKE_INSTALL_PREFIX}")
set(PKG_LIBDIR "${CMAKE_INSTALL_FULL_LIBDIR}")
set(PKG_INCLUDEDIR "${CMAKE_INSTALL_FULL_INCLUDEDIR}")
if(APPLE)
    set(PKG_PRIVATE_LIBS "-framework CoreFoundation -framework Security -framework SystemConfiguration -liconv -lobjc -lpthread")
elseif(WIN32)
    set(PKG_PRIVATE_LIBS "-lws2_32 -luserenv -lntdll -lbcrypt")
else()
    set(PKG_PRIVATE_LIBS "-lpthread -ldl -lm -lrt -lutil")
endif()
configure_file("${CMAKE_CURRENT_SOURCE_DIR}/rax.pc.in"
               "${CMAKE_CURRENT_BINARY_DIR}/rax.pc" @ONLY)
install(FILES "${CMAKE_CURRENT_BINARY_DIR}/rax.pc"
        DESTINATION "${CMAKE_INSTALL_LIBDIR}/pkgconfig")

# CMake package config so downstreams can `find_package(rax)`.
set(_rax_cmake_install_dir "${CMAKE_INSTALL_LIBDIR}/cmake/rax")
configure_package_config_file(
    "${CMAKE_CURRENT_SOURCE_DIR}/cmake/raxConfig.cmake.in"
    "${CMAKE_CURRENT_BINARY_DIR}/raxConfig.cmake"
    INSTALL_DESTINATION "${_rax_cmake_install_dir}")
write_basic_package_version_file(
    "${CMAKE_CURRENT_BINARY_DIR}/raxConfigVersion.cmake"
    VERSION ${PROJECT_VERSION}
    COMPATIBILITY SameMajorVersion)
install(FILES
    "${CMAKE_CURRENT_BINARY_DIR}/raxConfig.cmake"
    "${CMAKE_CURRENT_BINARY_DIR}/raxConfigVersion.cmake"
    DESTINATION "${_rax_cmake_install_dir}")

message(STATUS "rax: cargo=${CARGO_EXECUTABLE} profile=${RAX_CARGO_PROFILE} out=${_rax_out_dir}")
