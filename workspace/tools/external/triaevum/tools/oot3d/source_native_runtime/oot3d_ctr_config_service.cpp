#include "oot3d_ctr_config_service.h"

#include <cstring>
#include "triaevum/sha256.h"

namespace Oot3dSourceRuntime {
namespace {

constexpr std::uint32_t kGetConfigRequest = 0x00010082U;
constexpr std::uint32_t kGetConfigResponse = 0x00010042U;
constexpr std::uint32_t kGetRegionRequest = 0x00020000U;
constexpr std::uint32_t kGetRegionResponse = 0x00020080U;
constexpr std::uint32_t kSoundOutputModeBlockId = 0x00070001U;
constexpr std::uint32_t kLanguageBlockId = 0x000A0002U;
constexpr std::uint32_t kStereoCameraSettingsBlockId = 0x00050005U;

} // namespace

CtrConfigService::CtrConfigService(GuestAddressSpace& memory,
                                   CtrConfigServiceProfile profile)
    : mMemory(memory), mProfile(profile) {}

CtrResult CtrConfigService::Dispatch(std::span<std::uint32_t> commandBuffer) {
    if (commandBuffer.empty()) {
        return CtrIpcRouter::UnhandledResult;
    }
    // CTR hashes the little-endian console identifier then the masked 20-bit
    // application salt, returning the final eight SHA-256 bytes.
    if (commandBuffer[0] == 0x00030040U && commandBuffer.size() >= 4) {
        std::array<std::uint8_t,12> input{};
        for(unsigned i=0;i<8;++i)input[i]=mProfile.ConsoleUniqueId>>(i*8U);
        const auto salt=commandBuffer[1]&0xfffffU;
        for(unsigned i=0;i<4;++i)input[8+i]=salt>>(i*8U);
        const auto hash=triaevum::module::detail::Sha256(input);
        commandBuffer[0]=0x000300c0U;commandBuffer[1]=0;
        std::memcpy(&commandBuffer[2],hash.data()+24,8);
        return 0;
    }
    if (commandBuffer[0] == 0x00050000U && commandBuffer.size() >= 3) {
        commandBuffer[0] = 0x00050080U;
        commandBuffer[1] = 0;
        commandBuffer[2] = mProfile.SystemModel;
        return 0;
    }
    if (commandBuffer[0] == 0x00040000U && commandBuffer.size() >= 3) {
        commandBuffer[0] = 0x00040080U;
        commandBuffer[1] = 0;
        commandBuffer[2] = mProfile.SystemRegion == 1U &&
            (mProfile.CountryInfo[3] == 18U || mProfile.CountryInfo[3] == 49U);
        return 0;
    }
    if (commandBuffer[0] == 0x00060000U && commandBuffer.size() >= 3) {
        commandBuffer[0] = 0x00060080U;
        commandBuffer[1] = 0;
        commandBuffer[2] = mProfile.SystemModel == 2U ? 0U : 1U;
        return 0;
    }
    if (commandBuffer[0] == kGetRegionRequest && commandBuffer.size() >= 3) {
        commandBuffer[0] = kGetRegionResponse;
        commandBuffer[1] = 0;
        commandBuffer[2] = mProfile.SystemRegion;
        return 0;
    }
    if (commandBuffer[0] != kGetConfigRequest || commandBuffer.size() < 5) {
        return CtrIpcRouter::UnhandledResult;
    }

    const std::size_t size = commandBuffer[1];
    const std::uint32_t blockId = commandBuffer[2];
    const std::uint32_t descriptor = commandBuffer[3];
    const GuestAddress address = commandBuffer[4];
    const bool validDescriptor =
        (descriptor & 0x8U) != 0U && ((descriptor >> 1U) & 2U) != 0U &&
        (descriptor >> 4U) == size;
    if (!validDescriptor || !WriteConfigBlock(blockId, address, size)) {
        return CtrIpcRouter::UnhandledResult;
    }
    commandBuffer[0] = kGetConfigResponse;
    commandBuffer[1] = 0;
    commandBuffer[2] = descriptor;
    commandBuffer[3] = address;
    return 0;
}

bool CtrConfigService::WriteConfigBlock(std::uint32_t blockId,
                                        GuestAddress address,
                                        std::size_t size) {
    const void* source = nullptr;
    std::size_t sourceSize = 0;
    if (blockId == kSoundOutputModeBlockId) {
        source = &mProfile.SoundOutputMode;
        sourceSize = sizeof(mProfile.SoundOutputMode);
    } else if (blockId == 0x000A0000U) {
        source = mProfile.UserName.data();
        sourceSize = sizeof(mProfile.UserName);
    } else if (blockId == 0x000A0001U) {
        source = mProfile.Birthday.data();
        sourceSize = mProfile.Birthday.size();
    } else if (blockId == 0x00090000U) {
        source = &mProfile.CurrentLocalFriendCode;
        sourceSize = sizeof(mProfile.CurrentLocalFriendCode);
    } else if (blockId == kLanguageBlockId) {
        source = &mProfile.SystemLanguage;
        sourceSize = sizeof(mProfile.SystemLanguage);
    } else if (blockId == kStereoCameraSettingsBlockId) {
        source = mProfile.StereoCameraSettings.data();
        sourceSize = sizeof(mProfile.StereoCameraSettings);
    } else if (blockId == 0x000B0000U) {
        source = mProfile.CountryInfo.data();
        sourceSize = mProfile.CountryInfo.size();
    } else if (blockId == 0x000B0001U) {
        source = mProfile.CountryNames.data();
        sourceSize = sizeof(mProfile.CountryNames);
    } else if (blockId == 0x000B0002U) {
        source = mProfile.StateNames.data();
        sourceSize = sizeof(mProfile.StateNames);
    } else if (blockId == 0x000B0003U) {
        source = mProfile.Coordinates.data();
        sourceSize = sizeof(mProfile.Coordinates);
    } else if (blockId == 0x00030001U) {
        source = &mProfile.UserTimeOffset;
        sourceSize = sizeof(mProfile.UserTimeOffset);
    } else if (blockId == 0x000C0001U) {
        source = mProfile.CoppaRestrictions.data();
        sourceSize = mProfile.CoppaRestrictions.size();
    } else if (blockId == 0x000C0000U) {
        source = mProfile.ParentalControls.data();
        sourceSize = mProfile.ParentalControls.size();
    }
    if (source == nullptr || sourceSize != size) {
        return false;
    }
    auto destination = mMemory.ResolveWrite(address, size);
    if (destination.size() != size) {
        return false;
    }
    std::memcpy(destination.data(), source, size);
    return true;
}

} // namespace Oot3dSourceRuntime
