#pragma once

#include "oot3d_ctr_ipc_router.h"
#include <array>

namespace Oot3dSourceRuntime {

class CtrNdmService final : public CtrIpcSession {
  public:
    CtrResult Dispatch(std::span<std::uint32_t> commandBuffer) override;
    bool SchedulerSuspended() const;
    bool RunsInBackground() const;

  private:
    bool mSchedulerSuspended = false;
    bool mRunsInBackground = false;
    std::uint32_t mDefaultDaemons = 9U; // CEC and Friends.
    std::uint32_t mExclusiveState = 0;
    std::uint32_t mExclusiveProcess = 0;
    std::array<std::uint32_t, 4> mDaemonSuspendCounts{};
};

} // namespace Oot3dSourceRuntime
