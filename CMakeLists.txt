cmake_minimum_required(VERSION 3.24)
project(WanderOverlay LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(UL_SDK_PATH "${CMAKE_CURRENT_SOURCE_DIR}/ul-sdk-2.0.0-beta.2-win-x64-free" CACHE PATH "Ultralight SDK")
find_package(Ultralight REQUIRED PATHS "${UL_SDK_PATH}/lib/cmake/Ultralight" NO_DEFAULT_PATH)
add_library(ul_driver STATIC
  "${UL_SDK_PATH}/platform/src/GPUDriver.cpp"
  "${UL_SDK_PATH}/platform/src/gpu/d3d11/GPUContextD3D11.cpp"
  "${UL_SDK_PATH}/platform/src/gpu/d3d11/GPUDriverD3D11.cpp"
  "${UL_SDK_PATH}/platform/src/gpu/d3d11/SwapChainD3D11.cpp")
target_include_directories(ul_driver PUBLIC "${UL_SDK_PATH}/platform/include" PRIVATE
  "${UL_SDK_PATH}/platform/src" "${UL_SDK_PATH}/platform/shaders/generated/headers")
target_compile_definitions(ul_driver PUBLIC DRIVER_D3D11 NOMINMAX UNICODE _UNICODE)
target_link_libraries(ul_driver PUBLIC Ultralight::AppCore d3d11 dxgi d3dcompiler)
add_library(overlay-platform SHARED native/main.cpp)
set_target_properties(overlay-platform PROPERTIES WINDOWS_EXPORT_ALL_SYMBOLS ON)
target_include_directories(overlay-platform PRIVATE vendor/openvr/headers)
target_link_libraries(overlay-platform PRIVATE ul_driver windowsapp dwmapi "${CMAKE_CURRENT_SOURCE_DIR}/vendor/openvr/lib/win64/openvr_api.lib")
ultralight_copy_runtime_files(overlay-platform)
add_custom_target(overlay-assets ALL
  COMMAND ${CMAKE_COMMAND} -E copy_if_different "${CMAKE_CURRENT_SOURCE_DIR}/vendor/openvr/bin/win64/openvr_api.dll" "$<TARGET_FILE_DIR:overlay-platform>"
  COMMAND ${CMAKE_COMMAND} -E copy_directory "${CMAKE_CURRENT_SOURCE_DIR}/ui" "$<TARGET_FILE_DIR:overlay-platform>/assets/ui"
  COMMAND ${CMAKE_COMMAND} -E copy_directory "${CMAKE_CURRENT_SOURCE_DIR}/input" "$<TARGET_FILE_DIR:overlay-platform>/assets/input"
  DEPENDS overlay-platform)
