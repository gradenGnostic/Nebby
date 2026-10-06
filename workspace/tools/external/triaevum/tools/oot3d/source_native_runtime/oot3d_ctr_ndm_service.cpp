#include "oot3d_ctr_ndm_service.h"

namespace Oot3dSourceRuntime {

CtrResult CtrNdmService::Dispatch(std::span<std::uint32_t> commandBuffer) {
    if (commandBuffer.size() >= 4 && commandBuffer[0] == 0x00010042U) {
        if (commandBuffer[1] > 4U || commandBuffer[2] != 0x20U ||
            (mExclusiveState != 0 && mExclusiveProcess != commandBuffer[3]))
            return CtrIpcRouter::UnhandledResult;
        mExclusiveState = commandBuffer[1];
        mExclusiveProcess = commandBuffer[3];
        commandBuffer[0] = 0x00010040U;
        commandBuffer[1] = 0;
        return 0;
    }
    if (commandBuffer.size() >= 3 && commandBuffer[0] == 0x00020002U) {
        if (commandBuffer[1] != 0x20U ||
            (mExclusiveState != 0 && mExclusiveProcess != commandBuffer[2]))
            return CtrIpcRouter::UnhandledResult;
        mExclusiveState = 0;
        mExclusiveProcess = 0;
        commandBuffer[0] = 0x00020040U;
        commandBuffer[1] = 0;
        return 0;
    }
    if (commandBuffer.size() >= 3 && commandBuffer[0] == 0x00030000U) {
        commandBuffer[0] = 0x00030080U;
        commandBuffer[1] = 0;
        commandBuffer[2] = mExclusiveState;
        return 0;
    }
    // Offline native runtime: daemon control still owns real suspension state.
    // Semantics checked against Azahar ndm_u.cpp (GPL-2.0-or-later).
    if (commandBuffer.size() >= 2 && commandBuffer[0] == 0x00140040U) {
        mDefaultDaemons = commandBuffer[1] & 0xFU;
        commandBuffer[1] = 0;
        return 0;
    }
    if (commandBuffer.size() >= 2 &&
        (commandBuffer[0] == 0x00060040U || commandBuffer[0] == 0x00070040U)) {
        const bool suspend = commandBuffer[0] == 0x00060040U;
        const auto mask = commandBuffer[1] & 0xFU;
        for (unsigned i=0;i<4;++i) {
            if ((mask & (1U<<i)) &&
                ((!suspend && mDaemonSuspendCounts[i] == 0) ||
                 (suspend && mDaemonSuspendCounts[i] == UINT32_MAX)))
                return CtrIpcRouter::UnhandledResult;
        }
        for (unsigned i=0;i<4;++i) if (mask & (1U<<i)) {
            if (suspend) ++mDaemonSuspendCounts[i];
            else --mDaemonSuspendCounts[i];
        }
        commandBuffer[1] = 0;
        return 0;
    }
    if (commandBuffer.size() >= 2 && commandBuffer[0] == 0x00080040U) {
        mSchedulerSuspended = true;
        mRunsInBackground = commandBuffer[1] != 0;
        commandBuffer[0] = 0x00080040U;
        commandBuffer[1] = 0;
        return 0;
    }
    if (commandBuffer.size() >= 2 && commandBuffer[0] == 0x00090000U) {
        mSchedulerSuspended = false;
        mRunsInBackground = false;
        commandBuffer[0] = 0x00090040U;
        commandBuffer[1] = 0;
        return 0;
    }
    return CtrIpcRouter::UnhandledResult;
}

bool CtrNdmService::SchedulerSuspended() const { return mSchedulerSuspended; }
bool CtrNdmService::RunsInBackground() const { return mRunsInBackground; }

} // namespace Oot3dSourceRuntime
