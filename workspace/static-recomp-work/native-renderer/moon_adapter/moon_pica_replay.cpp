// Diagnostic replay: feed real Moon command words into TriAevum's PICA
// frontend. This is not the runtime renderer or a visual success claim.
#include "oot3d_native_pica_frontend.h"

#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <string>
#include <vector>

namespace {
class MoonSink final : public Oot3dNativeGame::Oot3dPicaPacketSink {
  public:
    uint64_t draws = 0;
    bool SubmitHardwareRegisterWrite(
        const Oot3dNativeGame::Oot3dPicaHardwareRegisterWrite&,
        std::string*) override {
        return true;
    }
    bool SubmitDrawPacket(const Oot3dNativeGame::Oot3dPicaDrawPacket& packet,
                          std::string*) override {
        if (draws++ == 0)
            std::cout << "FIRST REAL MOON DRAW packet at 0x" << std::hex
                      << packet.CommandListAddress << std::dec
                      << " indexed=" << packet.Indexed
                      << " vertices=" << (packet.Registers[0x228] & 0xffff) << '\n';
        return true;
    }
};
} // namespace

int main(int argc, char** argv) {
    if (argc < 2) return 2;
    MoonSink sink;
    Oot3dNativeGame::Oot3dNativePicaFrontend frontend(&sink);
    frontend.SetDiagnosticHistoryEnabled(false);
    uint64_t lists = 0;
    for (int i = 1; i < argc; ++i) {
        const std::filesystem::path path(argv[i]);
        const auto stem = path.stem().string();
        const auto dash = stem.rfind('-');
        if (dash == std::string::npos) continue;
        const auto address = std::stoul(stem.substr(dash + 1), nullptr, 16);
        std::ifstream in(path, std::ios::binary | std::ios::ate);
        if (!in) continue;
        const auto size = in.tellg();
        if (size <= 0 || size > 1024 * 1024 || size % 4) continue;
        in.seekg(0);
        std::vector<uint32_t> words(static_cast<size_t>(size) / 4);
        in.read(reinterpret_cast<char*>(words.data()), size);
        Oot3dNativeGame::Oot3dGspCommandPacket command{};
        command.Control = 1;
        command.Parameters[0] = static_cast<uint32_t>(address);
        command.Parameters[1] = static_cast<uint32_t>(size);
        std::string error;
        if (!frontend.SubmitGspCommand(command, words, &error)) {
            std::cerr << path.filename() << ": " << error << '\n';
            continue;
        }
        ++lists;
    }
    std::cout << "moon_lists=" << lists << " triaevum_draw_packets=" << sink.draws << '\n';
    return sink.draws == 0 ? 1 : 0;
}
