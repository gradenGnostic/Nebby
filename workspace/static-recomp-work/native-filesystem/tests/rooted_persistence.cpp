#include "triaevum/rooted_filesystem_backend.h"
#include <array>
#include <cassert>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <unistd.h>
using namespace triaevum::module;
int main() {
  char temporary[] = "/tmp/ctr-native-fs-test-XXXXXX";
  auto directory = ::mkdtemp(temporary);
  assert(directory);
  const std::filesystem::path base(directory);
  RootedFilesystemConfigV1 config;
  config.saveRoot = base / "save";
  std::ofstream(base / "outside") << "untouched";
  auto fs = RootedFilesystemBackendV1::Create(config);
  assert(fs);
  FilesystemOpenResultV1 file;
  constexpr auto save = TRIAEVUM_FILESYSTEM_SAVE_V1;
  constexpr unsigned flags = TRIAEVUM_FILESYSTEM_OPEN_READ_V1 |
      TRIAEVUM_FILESYSTEM_OPEN_WRITE_V1 | TRIAEVUM_FILESYSTEM_OPEN_CREATE_V1;
  assert(fs->Open(save, "state", flags, &file) == 0);
  unsigned written = 0;
  const std::array<uint8_t, 4> bytes{1, 3, 5, 7};
  assert(fs->Write(file.handle, 5, bytes, &written) == 0 && written == 4);
  assert(fs->Flush(file.handle) == 0);
  assert(fs->Resize(file.handle, 12) == 0);
  assert(fs->Close(file.handle) == 0);
  assert(fs->CreateDirectory("archive") == 0);
  assert(fs->Open(save, "archive/data", flags, &file) == 0);
  assert(fs->Write(file.handle, 0, bytes, &written) == 0);
  assert(fs->FormatDirectory("archive") != 0); // live files cannot be invalidated
  assert(fs->Close(file.handle) == 0);
  std::vector<RootedDirectoryEntryV1> listing;
  assert(fs->ListDirectory("archive", &listing) == 0 && listing.size() == 1);
  assert(listing[0].name == "data" && listing[0].size == 4);
  assert(fs->FormatDirectory("archive") == 0);
  assert(fs->ListDirectory("archive", &listing) == 0 && listing.empty());
  bool recovered = false;
  for (const auto &entry : std::filesystem::directory_iterator(config.saveRoot)) {
    if (entry.path().filename().string().starts_with("archive.before-format-"))
      recovered = std::filesystem::file_size(entry.path() / "data") == 4;
  }
  assert(recovered);
  assert(fs->Commit() == 0);
  fs.reset();
  fs = RootedFilesystemBackendV1::Create(config);
  assert(fs && fs->Open(save, "state", flags, &file) == 0 && file.size == 12);
  std::array<uint8_t, 4> actual{};
  FilesystemReadResultV1 read;
  assert(fs->Read(file.handle, 5, actual, &read) == 0);
  assert(read.returnedSize == 4 && actual == bytes);
  assert(fs->Close(file.handle) == 0);
  assert(fs->RenameFile("state", "renamed") == 0);
  assert(fs->Open(save, "renamed", flags, &file) == 0 && file.size == 12);
  assert(fs->Read(file.handle, 5, actual, &read) == 0 && actual == bytes);
  assert(fs->Close(file.handle) == 0);
  for (const auto *path : {"../outside", "/tmp/escape", "a/../../outside", "a\\b"})
    assert(fs->Open(save, path, flags, &file) != 0);
  std::filesystem::create_symlink(base / "outside", config.saveRoot / "escape");
  assert(fs->Open(save, "escape", flags, &file) != 0);
  std::filesystem::create_directory_symlink(base, config.saveRoot / "escape-dir");
  assert(fs->Open(save, "escape-dir/outside", flags, &file) != 0);
  bool removed = false;
  assert(fs->RemoveFile(save, "renamed", &removed) == 0 && removed);
  std::cout << "PASS durable write/reopen/resize/remove/traversal/final-symlink/parent-symlink\n";
  std::cout << "Test artifacts retained: " << base << '\n';
}
