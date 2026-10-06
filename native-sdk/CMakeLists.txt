cmake_minimum_required(VERSION 3.26)
project(NebbyNativePica LANGUAGES C CXX)
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_C_STANDARD 23)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
find_package(SDL2 REQUIRED)
find_package(Vulkan REQUIRED)
get_filename_component(WORK_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../../.." ABSOLUTE)
set(DONOR "${WORK_ROOT}/tools/external/triaevum")
set(NEBBY_VENDOR "${WORK_ROOT}/static-recomp-work/native-renderer/vendor")
if(EXISTS "${NEBBY_VENDOR}/nri/CMakeLists.txt")
  set(THREE_DS_RECOMP_NRI_SOURCE_DIR "${NEBBY_VENDOR}/nri" CACHE PATH "Packaged patched NRI source" FORCE)
endif()
foreach(DEPENDENCY IN ITEMS prism imgui monocypher threadpool vma vulkan_headers)
  string(TOUPPER "${DEPENDENCY}" DEPENDENCY_UPPER)
  if(EXISTS "${NEBBY_VENDOR}/${DEPENDENCY}")
    set(FETCHCONTENT_SOURCE_DIR_${DEPENDENCY_UPPER} "${NEBBY_VENDOR}/${DEPENDENCY}" CACHE PATH "Packaged renderer dependency" FORCE)
  endif()
endforeach()
set(THREE_DS_RECOMP_ENABLE_VULKAN ON CACHE BOOL "Canonical Vulkan backend")
set(THREE_DS_RECOMP_ENABLE_NRI ON CACHE BOOL "NRI native backend")
set(THREE_DS_RECOMP_ENABLE_CACAO OFF CACHE BOOL "No optional effects" FORCE)
set(THREE_DS_RECOMP_ENABLE_SSSR OFF CACHE BOOL "No optional reflections" FORCE)
set(THREE_DS_RECOMP_ENABLE_NRI_UPSCALER_SDKS OFF CACHE BOOL "Native fidelity only" FORCE)
set(THREE_DS_RECOMP_ENABLE_D3D12_NGX_PROVIDER OFF CACHE BOOL "Vulkan only" FORCE)
set(NRI_ENABLE_NGX_SDK OFF CACHE BOOL "No optional DLSS SDK" FORCE)
set(NRI_ENABLE_NIS_SDK OFF CACHE BOOL "No optional upscale SDK" FORCE)
set(NRI_ENABLE_SHADERMAKE OFF CACHE BOOL "Use canonical shaderc path" FORCE)
add_compile_definitions(CONTROLLERBUTTONS_T=uint32_t ENABLE_OOT3D_VULKAN)
add_subdirectory("${DONOR}/runtime/triaevum_module" triaevum_module)
add_subdirectory("${DONOR}/runtime/three_ds_recomp" three_ds_recomp_runtime)
set(PICA "${DONOR}/tools/oot3d/native_pica_frontend")
set(NATIVE "${DONOR}/tools/oot3d/native_game_runtime")
set(CTR "${DONOR}/tools/oot3d/source_native_runtime")
add_library(moon_triaevum_frontend_native SHARED
  "${WORK_ROOT}/static-recomp-work/native-renderer/moon_adapter/moon_triaevum_frontend.cpp"
  "${WORK_ROOT}/static-recomp-work/native-renderer/moon_adapter/moon_pica_draw_submission.cpp"
  "${PICA}/oot3d_native_pica_draw_state.cpp"
  "${NATIVE}/oot3d_native_pica_submission.cpp"
  "${CTR}/oot3d_ctr_pica_backend.cpp"
  "${PICA}/oot3d_native_pica_frontend.cpp"
  "${PICA}/oot3d_native_pica_fragment_lighting_gen.cpp"
  "${PICA}/oot3d_native_pica_fragment_shader_gen.cpp"
  "${PICA}/oot3d_native_pica_proctex.cpp"
  "${PICA}/oot3d_native_pica_program_descriptor.cpp"
  "${PICA}/oot3d_native_pica_shader_gen.cpp"
  "${PICA}/oot3d_native_pica_transfer.cpp"
  "${NATIVE}/oot3d_native_pica_vulkan_plan.cpp"
  "${DONOR}/tools/oot3d/third_party/azahar_pica/video_core/shader/generator/glsl_shader_decompiler.cpp")
target_include_directories(moon_triaevum_frontend_native PRIVATE
  "${PICA}" "${NATIVE}" "${CTR}" "${DONOR}/runtime/three_ds_recomp/include"
  "${DONOR}/tools/oot3d/native_a32_runtime/upstream"
  "${DONOR}/tools/oot3d/third_party/azahar_pica")
target_compile_options(moon_triaevum_frontend_native PRIVATE -ffunction-sections -fdata-sections)
set_source_files_properties("${NATIVE}/oot3d_native_pica_submission.cpp" "${NATIVE}/oot3d_native_pica_vulkan_plan.cpp" "${PICA}/oot3d_native_pica_transfer.cpp" PROPERTIES COMPILE_OPTIONS "-fvisibility=hidden")
target_link_options(moon_triaevum_frontend_native PRIVATE -Wl,--gc-sections -Wl,--no-undefined)
target_link_libraries(moon_triaevum_frontend_native PRIVATE three_ds_recomp_runtime SDL2::SDL2 vulkan)
