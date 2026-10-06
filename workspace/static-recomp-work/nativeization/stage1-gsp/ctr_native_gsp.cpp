// Thin host ABI around TriAevum's title-neutral CTR GSP implementation.
// Donor provenance: GPL TriAevum tools/oot3d/source_native_runtime; headers and
// implementation remain in the donor checkout with their original notices.
#include "oot3d_ctr_gsp_service.h"
#include "oot3d_ctr_dsp_service.h"
#include "oot3d_native_a32_dsp_hle.h"
#include "oot3d_ctr_hid_producer.h"
#include "oot3d_ctr_config_service.h"
#include "oot3d_ctr_ndm_service.h"
#include "oot3d_ctr_apt_service.h"
#include "ctr_y2r_session.h"
#include "ctr_offline_sessions.h"
#include <sys/random.h>
#include <cerrno>
#include <deque>
#include <cstring>
#include <limits>
#include <cstdio>
using namespace Oot3dSourceRuntime;
using Read = bool(*)(void*, uint32_t, uint8_t*, size_t);
using Write = bool(*)(void*, uint32_t, const uint8_t*, size_t);
using Action = bool(*)(void*, uint32_t, uint32_t, const uint32_t*, size_t);
struct CallbackMemory final : GuestAddressSpace {
    struct Span { uint32_t address; std::vector<std::byte> bytes; bool writable; };
    void* context{}; Read read{}; Write write{};
    mutable std::deque<Span> spans;
    bool IsReadable(uint32_t address,size_t size) const override {
        // A null destination is an explicit permission/range probe, not a read.
        return read && read(context,address,nullptr,size);
    }
    std::span<std::byte> resolve(uint32_t address, size_t size, bool writable) const {
        if (!read || !size || size > (16U<<20) || uint64_t(address)+size > (uint64_t{1}<<32)) return {};
        if(writable&&(!write||!write(context,address,nullptr,size)))return {};
        for(auto& span:spans) if(span.address==address && span.bytes.size()==size && span.writable==writable) return span.bytes;
        auto& span=spans.emplace_back(Span{address,std::vector<std::byte>(size),writable});
        if(!read(context,address,reinterpret_cast<uint8_t*>(span.bytes.data()),size)){spans.pop_back();return {};}
        return span.bytes;
    }
    std::span<const std::byte> ResolveRead(uint32_t a,size_t s) const override{return resolve(a,s,false);}
    std::span<std::byte> ResolveWrite(uint32_t a,size_t s) override{return resolve(a,s,true);}
    bool flush(){for(const auto& s:spans)if(s.writable && !write(context,s.address,reinterpret_cast<const uint8_t*>(s.bytes.data()),s.bytes.size()))return false;return true;}
};
struct CallbackGpu final : CtrGpuBackend {
    void* context{}; Action action{}; std::vector<uint8_t> interrupts;
    bool WriteRegisters(uint32_t base,std::span<const uint32_t> values,std::span<const uint32_t> masks) override {
        std::vector<uint32_t> packet(values.begin(),values.end());packet.insert(packet.end(),masks.begin(),masks.end());
        return action && action(context,masks.empty()?0:1,base,packet.data(),packet.size());
    }
    bool SubmitCommand(const CtrGspCommand& c,std::span<const uint32_t>) override {
        static uint64_t counts[6]{};
        const auto id=c.Control&255U;
        if(id<6 && (++counts[id]==1 || counts[id]%1024==0))
            std::fprintf(stderr,"NATIVE_GSP_COMMAND id=%u count=%llu address=%08x size=%08x\n",id,(unsigned long long)counts[id],c.Parameters[0],c.Parameters[1]);
        if(!action || !action(context,2,c.Control,c.Parameters.data(),7))return false;
        switch(c.Control & 255U){case 0:interrupts.push_back(6);break;case 1:interrupts.push_back(5);break;
        case 2:if(c.Parameters[0] && c.Parameters[2]>c.Parameters[0])interrupts.push_back(0);if(c.Parameters[3] && c.Parameters[5]>c.Parameters[3])interrupts.push_back(1);break;
        case 3:case 4:interrupts.push_back(4);break;case 5:break;default:return false;}
        return true;
    }
    bool SetFramebuffer(const CtrGspFramebuffer& f) override {
        uint32_t args[]={f.ActiveBuffer,f.AddressLeft,f.AddressRight,f.Stride,f.Format,f.ShownBuffer};
        return action && action(context,3,f.Screen,args,6);
    }
    void SetLcdForceBlack(bool b) override {if(action)action(context,4,b,nullptr,0);}
    std::vector<uint8_t> TakeInterrupts() override {auto out=std::move(interrupts);interrupts.clear();return out;}
};
struct Runtime {
    CallbackMemory memory; CtrIpcRouter router; CallbackGpu gpu;
    CtrGspService service{memory,router,&gpu};
    std::shared_ptr<CtrKernelObject> event=std::make_shared<CtrKernelObject>();
    uint32_t eventHandle{};
    Runtime(){event->Kind=CtrKernelObjectKind::Event;router.OpenKernelObject(event,eventHandle);}
};
#define API extern "C" __attribute__((visibility("default")))
API void* ctr_native_gsp_create() noexcept {try{return new Runtime;}catch(...){return nullptr;}}
API void ctr_native_gsp_destroy(void* p) noexcept {delete static_cast<Runtime*>(p);}
struct ServicesRuntime {
    CallbackMemory memory;
    CtrIpcRouter router;
    std::vector<std::shared_ptr<CtrKernelObject>> objectIdentities;
    std::shared_ptr<CtrConfigService> cfg;
    uint32_t configHandle{};
    std::shared_ptr<CtrNdmService> ndm;
    uint32_t ndmHandle{};
    std::shared_ptr<CtrAptService> apt;
    uint32_t aptHandle{};
    std::shared_ptr<CtrY2rSession> y2r;
    uint32_t y2rHandle{};
    std::shared_ptr<CtrFriendsSession> friends;
    std::shared_ptr<CtrSslSession> ssl;
    std::shared_ptr<CtrOfflineUdsSession> uds;
    uint32_t friendsHandle{},sslHandle{},udsHandle{};
    ServicesRuntime(uint32_t region,uint32_t language,uint32_t model,uint64_t identity){
        CtrConfigServiceProfile profile;profile.SystemRegion=region;profile.SystemLanguage=language;profile.SystemModel=model;
        if(region==1){profile.CountryInfo[3]=49;profile.Coordinates={7083,-14024};}
        // Profile labels, not synthetic opaque configuration bytes. Each
        // language slot occupies 0x80 bytes; host profile currently uses English.
        const std::u16string country=region==1?u"United States":u"United Kingdom";
        for(unsigned i=0;i<16;++i)std::copy(country.begin(),country.end(),profile.CountryNames.begin()+i*64);
        profile.ConsoleUniqueId=identity;
        cfg=std::make_shared<CtrConfigService>(memory,profile);
        if(!router.RegisterPort("cfg:u",cfg) || router.OpenSession("cfg:u",cfg,configHandle)<0)throw std::runtime_error("native config service registration");
        ndm=std::make_shared<CtrNdmService>();
        if(!router.RegisterPort("ndm:u",ndm) || router.OpenSession("ndm:u",ndm,ndmHandle)<0)throw std::runtime_error("native NDM service registration");
        apt=std::make_shared<CtrAptService>(memory,router);
        if(!router.RegisterPort("APT:A",apt) || router.OpenSession("APT:A",apt,aptHandle)<0)throw std::runtime_error("native APT service registration");
        y2r=std::make_shared<CtrY2rSession>(memory,router);
        if(!router.RegisterPort("y2r:u",y2r) || router.OpenSession("y2r:u",y2r,y2rHandle)<0)throw std::runtime_error("native Y2R service registration");
        friends=std::make_shared<CtrFriendsSession>(memory);
        ssl=std::make_shared<CtrSslSession>(memory,[](std::span<std::byte> bytes){
            // Linux host entropy adapter; the CTR service itself is portable.
            while(!bytes.empty()){
                const auto n=::getrandom(bytes.data(),bytes.size(),0);
                if(n<0&&errno==EINTR)continue;
                if(n<=0)return false;bytes=bytes.subspan(n);
            }return true;
        });
        uds=std::make_shared<CtrOfflineUdsSession>();
        if(!router.RegisterPort("frd:u",friends)||router.OpenSession("frd:u",friends,friendsHandle)<0||
            !router.RegisterPort("ssl:C",ssl)||router.OpenSession("ssl:C",ssl,sslHandle)<0||
            !router.RegisterPort("nwm::UDS",uds)||router.OpenSession("nwm::UDS",uds,udsHandle)<0)
            throw std::runtime_error("native offline service registration");
    }
};
API void* ctr_native_services_create(uint32_t region,uint32_t language,uint32_t model,uint64_t identity) noexcept {try{return new ServicesRuntime(region,language,model,identity);}catch(...){return nullptr;}}
API void ctr_native_services_destroy(void* p) noexcept {delete static_cast<ServicesRuntime*>(p);}
API uint32_t ctr_native_services_object_identity(void* p,uint32_t handle) noexcept {
    try{auto& runtime=*static_cast<ServicesRuntime*>(p);auto object=runtime.router.KernelObject(handle);
        if(!object)return 0;
        for(size_t i=0;i<runtime.objectIdentities.size();++i)
            if(runtime.objectIdentities[i]==object)return static_cast<uint32_t>(i+1);
        runtime.objectIdentities.push_back(std::move(object));
        return static_cast<uint32_t>(runtime.objectIdentities.size());
    }catch(...){return 0;}
}
API int32_t ctr_native_services_object(void* p,uint32_t handle,uint32_t* metadata) noexcept {
    try{auto object=static_cast<ServicesRuntime*>(p)->router.KernelObject(handle);
        if(!object||!metadata)return -1;
        std::lock_guard lock(object->StateMutex);
        metadata[0]=static_cast<uint32_t>(object->Kind);metadata[1]=object->AvailableCount;metadata[2]=object->MaximumCount;
        return 0;
    }catch(...){return -1;}
}
API int32_t ctr_native_services_dispatch(void* p,uint32_t service,uint32_t* words,size_t count,void* context,Read read,Write write,uint32_t staticTable) noexcept {
    try{auto& r=*static_cast<ServicesRuntime*>(p);r.memory.spans.clear();r.memory.context=context;r.memory.read=read;r.memory.write=write;
        struct ClearBorrow { CallbackMemory& memory; ~ClearBorrow(){memory.spans.clear();memory.context=nullptr;memory.read=nullptr;memory.write=nullptr;} } clearBorrow{r.memory};
        if(service>6)return INT32_MIN;
        r.apt->SetStaticBufferTableAddress(staticTable);
        r.friends->SetStaticBufferTableAddress(staticTable);
        const uint32_t handles[]={r.configHandle,r.ndmHandle,r.aptHandle,r.y2rHandle,r.friendsHandle,r.sslHandle,r.udsHandle};
        const auto handle=handles[service];
        const auto result=r.router.SendSyncRequest(handle,{words,count});
        if(!r.memory.flush())return INT32_MIN;r.memory.spans.clear();r.memory.context=nullptr;return result;
    }catch(...){return INT32_MIN;}
}
struct HidRuntime {
    CtrIpcRouter router;
    CtrHidService service{router};
    CtrHidProducer producer{service};
    std::array<std::shared_ptr<CtrKernelObject>,5> events;
};
API void* ctr_native_hid_create() noexcept {try{return new HidRuntime;}catch(...){return nullptr;}}
API void ctr_native_hid_destroy(void* p) noexcept {delete static_cast<HidRuntime*>(p);}
API int32_t ctr_native_hid_dispatch(void* p,uint32_t* words,size_t count) noexcept {
    try{auto& r=*static_cast<HidRuntime*>(p);const auto request=words[0];const auto result=r.service.Dispatch({words,count});
        if(result>=0 && request==0x000a0000U)for(size_t i=0;i<5;++i)r.events[i]=r.router.KernelObject(words[4+i]);
        return result;
    }catch(...){return INT32_MIN;}
}
API int32_t ctr_native_hid_sample(void* p,uint32_t buttons,int16_t x,int16_t y,uint16_t tx,uint16_t ty,
    uint32_t touched,uint64_t tick,uint8_t* output,size_t capacity,uint32_t* eventMask) noexcept {
    try{auto& r=*static_cast<HidRuntime*>(p);auto bytes=r.service.SharedMemoryBytes();
        if(bytes.empty())return 0;
        if(bytes.size()>capacity || !r.producer.Submit({buttons,x,y,tx,ty,touched!=0},tick))return INT32_MIN;
        std::memcpy(output,bytes.data(),bytes.size());*eventMask=0;
        for(size_t i=0;i<r.events.size();++i)if(r.events[i]&&r.events[i]->AvailableCount){*eventMask|=1U<<i;r.events[i]->AvailableCount=0;}
        return static_cast<int32_t>(bytes.size());
    }catch(...){return INT32_MIN;}
}
struct DspRuntime {
    CallbackMemory memory;
    CtrIpcRouter router;
    CtrDspService service{memory,router};
    Oot3dNativeGame::NativeA32DspHle mixer{{}};
    std::vector<std::pair<uint32_t,std::shared_ptr<CtrKernelObject>>> events;
    uint32_t guestSemaphore{};
};
using Physical = const uint8_t*(*)(void*,uint32_t);
API void* ctr_native_dsp_create() noexcept {try{return new DspRuntime;}catch(...){return nullptr;}}
API void ctr_native_dsp_destroy(void* p) noexcept {delete static_cast<DspRuntime*>(p);}
// Signal results are guest handles; only the host kernel mirror translates them.
API int32_t ctr_native_dsp_dispatch(void* p,uint32_t* words,size_t count,uint32_t table,
    uint32_t guestSemaphore,void* context,Read read,Write write,uint32_t* signals,size_t capacity,uint32_t* running) noexcept {
    try {
        auto& r=*static_cast<DspRuntime*>(p);r.memory.spans.clear();r.memory.context=context;r.memory.read=read;r.memory.write=write;
        struct ClearBorrow { CallbackMemory& memory; ~ClearBorrow(){memory.spans.clear();memory.context=nullptr;memory.read=nullptr;memory.write=nullptr;} } clearBorrow{r.memory};
        r.service.SetStaticBufferTableAddress(table);
        const uint32_t request=words[0];
        if(request==0x00150082U && words[4]) {
            const uint32_t guest=words[4];auto event=std::make_shared<CtrKernelObject>();event->Kind=CtrKernelObjectKind::Event;
            uint32_t handle{};if(r.router.OpenKernelObject(event,handle)<0)return INT32_MIN;
            r.events.emplace_back(guest,event);words[4]=handle;
        }
        const auto result=r.service.Dispatch({words,count});
        if(request==0x00160000U && result>=0){r.guestSemaphore=guestSemaphore;auto event=r.router.KernelObject(words[3]);r.events.emplace_back(guestSemaphore,event);words[3]=guestSemaphore;}
        if(!r.memory.flush())return INT32_MIN;
        size_t n=0;for(auto& [guest,event]:r.events)if(event && event->AvailableCount && n<capacity){signals[n++]=guest;event->AvailableCount=0;}
        if(n<capacity)signals[n]=0;
        *running=r.service.AudioRunning();r.memory.spans.clear();r.memory.context=nullptr;return result;
    }catch(...){return INT32_MIN;}
}
API int32_t ctr_native_dsp_frame(void* p,void* context,Read read,Write write,Physical physical,
    int16_t* pcm,size_t capacity,uint32_t* signals,size_t signalCapacity) noexcept {
    try {
        auto& r=*static_cast<DspRuntime*>(p);if(!r.service.AudioRunning())return 0;
        Oot3dNativeGame::NativeDspMemoryAccess access;
        access.Read=[&](uint32_t a,std::span<uint8_t> b){return read(context,a,b.data(),b.size());};
        access.Write=[&](uint32_t a,std::span<const uint8_t> b){return write(context,a,b.data(),b.size());};
        access.ResolvePhysical=[&](uint32_t a){return physical(context,a);};
        std::vector<int16_t> output;std::string error;
        if(!r.mixer.ProcessFrame(access,output,&error)){std::fprintf(stderr,"NATIVE_DSP_FRAME_ERROR %s\n",error.c_str());return INT32_MIN;}
        if(output.size()>capacity)return INT32_MIN;
        std::copy(output.begin(),output.end(),pcm);r.service.OnAudioFrame();
        size_t n=0;for(auto& [guest,event]:r.events)if(event && event->AvailableCount && n<signalCapacity){signals[n++]=guest;event->AvailableCount=0;}
        if(n<signalCapacity)signals[n]=0;return static_cast<int32_t>(output.size());
    }catch(...){return INT32_MIN;}
}
// Callbacks/spans live exclusively within this synchronous dispatch.
API int32_t ctr_native_gsp_dispatch(void* p,uint32_t* words,size_t count,uint32_t mapped,
    uint32_t tick,void* context,Read read,Write write,Action action,uint32_t* signal) noexcept {
    try {
        if(!p || !signal || (!tick && (!words || count<2)))return INT32_MIN;
        auto& r=*static_cast<Runtime*>(p);r.memory.spans.clear();r.memory.context=context;r.memory.read=read;r.memory.write=write;
        r.gpu.context=context;r.gpu.action=action;
        struct Clear {Runtime& r;~Clear(){r.memory.context=nullptr;r.memory.spans.clear();r.gpu.context=nullptr;}} clear{r};
        if(auto shared=r.service.SharedMemory();shared && mapped){shared->MappedMemory=&r.memory;shared->MappedAddress=mapped;}
        static unsigned samples=0;
        if(!tick && words[0]==0x000c0000U && mapped && samples++<8){auto q=r.memory.ResolveRead(mapped+0x800,0x200);if(q.size()==0x200){uint32_t h,c;std::memcpy(&h,q.data(),4);const auto i=h&255U;if(i<15){std::memcpy(&c,q.data()+0x20+i*0x20,4);std::fprintf(stderr,"NATIVE_GSP_QUEUE header=%08x control=%08x\n",h,c);}}}
        if(!tick && words[0]==0x00130042U && count>=5)words[3]=r.eventHandle;
        const auto result=tick?(r.service.OnVBlank()?0:INT32_MIN):r.service.Dispatch({words,count});
        if(!r.memory.flush())return INT32_MIN;
        *signal=r.event->AvailableCount;r.event->AvailableCount=0;
        return result;
    }catch(...){return INT32_MIN;}
}
