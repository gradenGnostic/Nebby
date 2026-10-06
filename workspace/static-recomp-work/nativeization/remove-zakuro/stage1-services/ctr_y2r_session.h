// Adapter for TriAevum's portable Citra/Azahar-derived Y2R implementation.
// GPL-2.0-or-later. Donor copyright/provenance retained in y2r_service.h.
#pragma once
#include "oot3d_ctr_ipc_router.h"
#include "y2r_service.h"
#include <cstring>
#include <stdexcept>

namespace Oot3dSourceRuntime {
class CtrY2rSession final : public CtrIpcSession {
    struct Memory {
        GuestAddressSpace& guest;
        bool IsWritable(uint32_t address,size_t size) {
            return guest.ResolveWrite(address,size).size()==size;
        }
        bool ReadBytes(uint32_t address, std::span<uint8_t> bytes) {
            const auto source=guest.ResolveRead(address,bytes.size());
            if(source.size()!=bytes.size())return false;
            std::memcpy(bytes.data(),source.data(),bytes.size());return true;
        }
        bool WriteBytes(uint32_t address, std::span<const uint8_t> bytes) {
            auto target=guest.ResolveWrite(address,bytes.size());
            if(target.size()!=bytes.size())return false;
            std::memcpy(target.data(),bytes.data(),bytes.size());return true;
        }
    };
    Memory mMemory;
    CtrIpcRouter& mRouter;
    CtrServices::Y2rState mState;
    std::shared_ptr<CtrKernelObject> mEvent;
    CtrHandle mEventHandle{};
  public:
    CtrY2rSession(GuestAddressSpace& memory,CtrIpcRouter& router)
        :mMemory{memory},mRouter(router),mEvent(std::make_shared<CtrKernelObject>()) {
        mEvent->Kind=CtrKernelObjectKind::Event;mEvent->Name="y2r:completion";
        if(mRouter.OpenKernelObject(mEvent,mEventHandle)<0)throw std::runtime_error("Y2R event");
    }
    CtrResult Dispatch(std::span<uint32_t> words) override {
        if(words.empty())return CtrIpcRouter::UnhandledResult;
        const uint16_t command=words[0]>>16;
        const auto normal=(words[0]>>6)&63U,translated=words[0]&63U;
        unsigned expected=0;
        switch(command){
        case 1:case 3:case 5:case 7:case 9:case 11:case 13:
        case 0x1a:case 0x1c:case 0x20:case 0x21:case 0x22:expected=1;break;
        case 0x10:case 0x11:case 0x12:case 0x13:case 0x18:case 0x1e:expected=4;break;
        case 0x24:expected=8;break;
        case 0x29:expected=3;break;
        default:break;
        }
        const bool dma=(command>=0x10&&command<=0x13)||command==0x18;
        if(normal!=expected||translated!=(dma?2U:0U)||words.size()<1U+normal+translated||
            (dma&&words[5]!=0U))return CtrIpcRouter::UnhandledResult;
        auto reply=CtrServices::DispatchY2r(mState,command,words.subspan(1,normal),mMemory);
        if(!reply.Handled)return CtrIpcRouter::UnhandledResult;
        if(reply.Clear)mRouter.ClearEvent(mEventHandle);
        if(reply.Signal)mRouter.SignalObject(mEventHandle);
        const auto normalReply=reply.Words.size();
        if(reply.EventHandle){
            CtrHandle handle{};
            if(mRouter.OpenKernelObject(mEvent,handle)<0)return CtrIpcRouter::UnhandledResult;
            reply.Words.push_back(0);reply.Words.push_back(handle);
        }
        if(words.size()<1+reply.Words.size())return CtrIpcRouter::UnhandledResult;
        words[0]=(uint32_t(command)<<16)|(uint32_t(normalReply)<<6)|(reply.EventHandle?2U:0U);
        std::copy(reply.Words.begin(),reply.Words.end(),words.begin()+1);
        return 0;
    }
};
}
