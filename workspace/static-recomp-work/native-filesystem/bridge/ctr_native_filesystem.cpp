// Local CTR filesystem ABI adapter. Uses TriAevum's title-neutral rooted
// filesystem implementation; provenance and licenses are recorded alongside it.
#include "triaevum/rooted_filesystem_backend.h"
#include <cstdio>
#include <memory>
#include <string>

using namespace triaevum::module;
extern "C" {
void *ctr_native_fs_create(const char *saveRoot, const char *contentImage) {
  try {
    if (!saveRoot) return nullptr;
    RootedFilesystemConfigV1 config;
    config.saveRoot = saveRoot;
    if (contentImage && *contentImage) config.contentFiles.emplace("image", contentImage);
    std::string error;
    auto backend = RootedFilesystemBackendV1::Create(std::move(config), &error);
    if (!backend) std::fprintf(stderr, "NATIVE_FS_INIT_ERROR %s\n", error.c_str());
    return backend.release();
  } catch (...) { return nullptr; }
}
void ctr_native_fs_destroy(void *context) {
  delete static_cast<RootedFilesystemBackendV1 *>(context);
}
int ctr_native_fs_open(void *context, unsigned root, const char *path,
                       unsigned flags, uint64_t *handle, uint64_t *size) {
  try {
    if (!context || !path || !handle || !size) return -1;
    FilesystemOpenResultV1 result;
    int status = static_cast<RootedFilesystemBackendV1 *>(context)->Open(
        root, path, flags, &result);
    if (!status) { *handle = result.handle; *size = result.size; }
    return status;
  } catch (...) { return -1; }
}
int ctr_native_fs_read(void *context, uint64_t handle, uint64_t offset,
                       uint8_t *data, unsigned size, unsigned *count) {
  try {
    if (!context || !count || (!data && size)) return -1;
    FilesystemReadResultV1 result;
    int status = static_cast<RootedFilesystemBackendV1 *>(context)->Read(
        handle, offset, {data, size}, &result);
    if (!status) *count = result.returnedSize;
    return status;
  } catch (...) { return -1; }
}
int ctr_native_fs_write(void *context, uint64_t handle, uint64_t offset,
                        const uint8_t *data, unsigned size, unsigned *count) {
  try {
    if (!context || !count || (!data && size)) return -1;
    return static_cast<RootedFilesystemBackendV1 *>(context)->Write(
        handle, offset, {data, size}, count);
  } catch (...) { return -1; }
}
int ctr_native_fs_resize(void *context, uint64_t handle, uint64_t size) {
  try {
    return context ? static_cast<RootedFilesystemBackendV1 *>(context)->Resize(handle, size) : -1;
  } catch (...) { return -1; }
}
int ctr_native_fs_flush(void *context, uint64_t handle) {
  try {
    return context ? static_cast<RootedFilesystemBackendV1 *>(context)->Flush(handle) : -1;
  } catch (...) { return -1; }
}
int ctr_native_fs_commit(void *context) {
  try {
    return context ? static_cast<RootedFilesystemBackendV1 *>(context)->Commit() : -1;
  } catch (...) { return -1; }
}
int ctr_native_fs_close(void *context, uint64_t handle) {
  try {
    return context ? static_cast<RootedFilesystemBackendV1 *>(context)->Close(handle) : -1;
  } catch (...) { return -1; }
}
int ctr_native_fs_format(void *context, const char *path) {
  try { return context && path ? static_cast<RootedFilesystemBackendV1 *>(context)->FormatDirectory(path) : -1; }
  catch (...) { return -1; }
}
int ctr_native_fs_mkdir(void *context, const char *path) {
  try { return context && path ? static_cast<RootedFilesystemBackendV1 *>(context)->CreateDirectory(path) : -1; }
  catch (...) { return -1; }
}
int ctr_native_fs_rename(void *context, const char *source, const char *destination) {
  try { return context && source && destination ? static_cast<RootedFilesystemBackendV1 *>(context)->RenameFile(source,destination) : -1; }
  catch (...) { return -1; }
}
int ctr_native_fs_remove(void *context, const char *path) {
  try {
    if (!context || !path) return -1;
    bool removed = false;
    const int status = static_cast<RootedFilesystemBackendV1 *>(context)->RemoveFile(TRIAEVUM_FILESYSTEM_SAVE_V1,path,&removed);
    return !status && !removed ? static_cast<int>(TRIAEVUM_MODULE_TITLE_ERROR_V1) : status;
  } catch (...) { return -1; }
}
int ctr_native_fs_list(void *context, const char *path,
    void (*entry)(void *, const char *, unsigned, uint64_t), void *user) {
  try {
    if (!context || !path || !entry) return -1;
    std::vector<RootedDirectoryEntryV1> entries;
    int status = static_cast<RootedFilesystemBackendV1 *>(context)->ListDirectory(path,&entries);
    if (!status) for (const auto &file : entries) entry(user,file.name.c_str(),file.directory,file.size);
    return status;
  } catch (...) { return -1; }
}
}
