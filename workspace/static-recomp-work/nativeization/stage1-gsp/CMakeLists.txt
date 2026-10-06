cmake_minimum_required(VERSION 3.26)
project(ctr_native_gsp LANGUAGES CXX)
find_package(nlohmann_json 3.2.0 QUIET)
if(NOT TARGET nlohmann_json::nlohmann_json)
  find_path(CTR_JSON_INCLUDE_DIR nlohmann/json.hpp REQUIRED)
endif()
set(CMAKE_CXX_STANDARD 20)
get_filename_component(WORK_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
set(DONOR "${WORK_ROOT}/tools/external/triaevum/tools/oot3d/source_native_runtime")
add_library(ctr_native_gsp SHARED ctr_native_gsp.cpp ctr_native_memory.cpp
    ${WORK_ROOT}/tools/external/triaevum/runtime/triaevum_module/src/sha256.cpp
    ${DONOR}/oot3d_ctr_dsp_service.cpp
    ${DONOR}/oot3d_ctr_config_service.cpp
    ${DONOR}/oot3d_ctr_apt_service.cpp
    ${DONOR}/oot3d_ctr_ndm_service.cpp
    ${DONOR}/oot3d_ctr_hid_service.cpp
    ${DONOR}/oot3d_ctr_hid_producer.cpp
    ${DONOR}/../native_game_runtime/oot3d_native_a32_dsp_hle.cpp
    ${DONOR}/../third_party/azahar_audio/audio_core/codec.cpp
    ${DONOR}/../third_party/azahar_audio/audio_core/interpolate.cpp
    ${DONOR}/../third_party/azahar_audio/audio_core/hle/filter.cpp
    ${DONOR}/../third_party/azahar_audio/audio_core/hle/mixers.cpp
    ${DONOR}/../third_party/azahar_audio/audio_core/hle/source.cpp
    ${DONOR}/oot3d_ctr_gsp_service.cpp ${DONOR}/oot3d_ctr_ipc_router.cpp
    ${DONOR}/oot3d_guest_address_space.cpp
    ${DONOR}/oot3d_source_execution_stack.cpp
    ${DONOR}/oot3d_ctr_host_services.cpp
    ${DONOR}/oot3d_source_data_bindings.cpp
    ${DONOR}/oot3d_source_function_registry.cpp)
target_include_directories(ctr_native_gsp PRIVATE ${DONOR}
    ${WORK_ROOT}/tools/external/triaevum/tools/ctr_services
    ${CMAKE_CURRENT_SOURCE_DIR}/../remove-zakuro/stage1-services
    ${WORK_ROOT}/tools/external/triaevum/runtime/triaevum_module/include
    ${DONOR}/../native_game_runtime ${DONOR}/../third_party/azahar_audio
    ${CTR_JSON_INCLUDE_DIR})
if(TARGET nlohmann_json::nlohmann_json)
  target_link_libraries(ctr_native_gsp PRIVATE nlohmann_json::nlohmann_json)
endif()
target_compile_options(ctr_native_gsp PRIVATE -ffunction-sections -fdata-sections -fvisibility=hidden)
target_link_options(ctr_native_gsp PRIVATE -Wl,--gc-sections -Wl,--no-undefined)
