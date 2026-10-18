cmake_minimum_required(VERSION 3.22.1)
project(xmtp_react_native LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(ReactAndroid REQUIRED CONFIG)
find_package(fbjni REQUIRED CONFIG)
add_library(xmtp_sdk SHARED IMPORTED)
# Rust cdylibs omit SONAME; link by name so DT_NEEDED has no build-machine path.
set_target_properties(xmtp_sdk PROPERTIES
  IMPORTED_NO_SONAME TRUE
  IMPORTED_LOCATION "${CMAKE_CURRENT_SOURCE_DIR}/src/main/jniLibs/${ANDROID_ABI}/libxmtp_sdk.so")

add_library(xmtp_react_native SHARED
  XmtpJni.cpp
  ../cpp/XmtpNative.cpp
  ../cpp/XmtpRuntime.cpp
  ../cpp/generated/xmtp_sdk.cpp)
target_include_directories(xmtp_react_native PRIVATE ../cpp ../cpp/generated ../cpp/runtime)
target_compile_options(xmtp_react_native PRIVATE -fexceptions -frtti -Wall -Wextra)
target_link_options(xmtp_react_native PRIVATE "-Wl,-z,max-page-size=16384")
target_link_libraries(xmtp_react_native
  xmtp_sdk ReactAndroid::reactnative ReactAndroid::jsi fbjni::fbjni log)
