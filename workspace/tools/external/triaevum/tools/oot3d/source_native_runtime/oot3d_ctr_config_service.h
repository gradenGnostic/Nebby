#pragma once

#include "oot3d_ctr_ipc_router.h"
#include "oot3d_guest_address_space.h"

#include <array>
#include <cstdint>

namespace Oot3dSourceRuntime {

struct CtrConfigServiceProfile {
    std::uint8_t SoundOutputMode = 1;
    std::uint8_t SystemLanguage = 1;
    std::uint8_t SystemRegion = 2;
    std::uint8_t SystemModel = 0;
    std::uint64_t ConsoleUniqueId = 0;
    std::uint64_t CurrentLocalFriendCode = 0; // No registered Friends account.
    std::array<char16_t, 14> UserName{u'N',u'e',u'b',u'b',u'y'};
    // Virtual console birthday, month followed by day.
    std::array<std::uint8_t, 2> Birthday{1, 1};
    // Virtual console location: reserved, reserved, state, country (UK).
    std::array<std::uint8_t, 4> CountryInfo{0, 0, 0, 110};
    std::array<char16_t, 1024> CountryNames{};
    // State code zero denotes no selected subdivision, with empty labels.
    std::array<char16_t, 1024> StateNames{};
    // Signed latitude/longitude, degrees * 32768/180; profile default London.
    std::array<std::int16_t, 2> Coordinates{9377, -23};
    // Difference between virtual console RTC and displayed time, milliseconds.
    std::int64_t UserTimeOffset = 0;
    // New virtual console has no configured COPPA restrictions.
    std::array<std::uint8_t, 20> CoppaRestrictions{};
    std::array<std::uint8_t, 192> ParentalControls = [] {
        std::array<std::uint8_t, 192> value{};
        value[9] = 20; // Unrestricted age; global restrictions bit is clear.
        return value;
    }();
    std::array<float, 8> StereoCameraSettings{
        62.0F, 289.0F, 76.80000305175781F, 46.08000183105469F,
        10.0F, 5.0F, 55.58000183105469F, 21.56999969482422F,
    };
};

class CtrConfigService final : public CtrIpcSession {
  public:
    CtrConfigService(GuestAddressSpace& memory,
                     CtrConfigServiceProfile profile = {});

    CtrResult Dispatch(std::span<std::uint32_t> commandBuffer) override;

  private:
    bool WriteConfigBlock(std::uint32_t blockId, GuestAddress address,
                          std::size_t size);

    GuestAddressSpace& mMemory;
    CtrConfigServiceProfile mProfile;
};

} // namespace Oot3dSourceRuntime
