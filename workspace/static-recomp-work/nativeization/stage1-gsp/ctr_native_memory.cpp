// Title-neutral process memory owner. Physical stores use TriAevum's owned
// GuestAddressSpace; virtual mappings alias them without relocating guest PCs.
#include "oot3d_guest_address_space.h"
#include <map>
#include <array>
#include <algorithm>
#include <cstdio>
#include <stdexcept>
#include <atomic>
using namespace Oot3dSourceRuntime;
struct Mapping { uint32_t base,size,physical,permission,state; };
struct NativeMemory {
    MappedGuestAddressSpace stores;
    std::map<uint32_t,Mapping> mappings;
    std::vector<uint8_t> allocated;
    std::array<std::pair<uint32_t,uint32_t>,3> regions;
    uint64_t calls=0;
    NativeMemory(uint32_t total,uint32_t application) {
        if(total!=0x08000000U && total!=0x10000000U)throw std::invalid_argument("invalid CTR memory size");
        const auto rw=GuestMemoryAccess::Read|GuestMemoryAccess::Write;
        if(!stores.MapZeroed(0x18000000U,0x600000U,rw,"ctr_vram") ||
           !stores.MapZeroed(0x1ff00000U,0x80000U,rw,"ctr_dsp_ram") ||
           !stores.MapZeroed(0x1ff80000U,0x80000U,rw,"ctr_axi_wram") ||
           !stores.MapZeroed(0x20000000U,total,rw,"ctr_fcram"))throw std::bad_alloc();
        application=std::min(application,total);const auto system=std::min(total-application,0x2000000U);
        allocated.resize(total/0x1000);regions={{{0,application/0x1000},{application/0x1000,system/0x1000},{(application+system)/0x1000,(total-application-system)/0x1000}}};
        static std::atomic_flag announced=ATOMIC_FLAG_INIT;
        if(!announced.test_and_set())std::fprintf(stderr,"MEMORY_BACKEND=native (TriAevum owned address space, Moon metadata mappings)\n");
    }
    ~NativeMemory(){std::fprintf(stderr,"NATIVE_MEMORY_COUNTERS zakuro_memory_calls=0 native_memory_calls=%llu mappings=%zu\n",(unsigned long long)calls,mappings.size());}
};
#define API extern "C" __attribute__((visibility("default")))
API void* ctr_native_memory_create(uint32_t total,uint32_t application) noexcept {try{return new NativeMemory(total,application);}catch(...){return nullptr;}}
API void ctr_native_memory_destroy(void* p) noexcept {delete static_cast<NativeMemory*>(p);}
API uint8_t* ctr_native_memory_physical(void* p,uint32_t address,uint32_t size) noexcept {
    try{auto& r=*static_cast<NativeMemory*>(p);++r.calls;auto span=r.stores.ResolveWrite(address,size);return span.size()==size?reinterpret_cast<uint8_t*>(span.data()):nullptr;}catch(...){return nullptr;}
}
API uint32_t ctr_native_memory_region(void* p,uint32_t region,uint32_t used) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;if(region>=3)return 0;auto[start,count]=r.regions[region];
    return used?static_cast<uint32_t>(std::count(r.allocated.begin()+start,r.allocated.begin()+start+count,1))*0x1000U:count*0x1000U;
}
API uint32_t ctr_native_memory_allocate(void* p,uint32_t region,uint32_t size) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;if(region>=3||size>0xfffff000U)return 0;
    const auto needed=(size+0xfffU)/0x1000U;if(!needed)return 0x20000000U;
    auto[start,count]=r.regions[region];uint32_t run=0;
    for(uint32_t i=start;i<start+count;++i){run=r.allocated[i]?0:run+1;if(run==needed){const auto first=i+1-needed;std::fill(r.allocated.begin()+first,r.allocated.begin()+first+needed,1);return 0x20000000U+first*0x1000U;}}
    return 0;
}
API bool ctr_native_memory_free(void* p,uint32_t address,uint32_t size) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;if(address<0x20000000U || address%0x1000U || size%0x1000U)return false;
    const auto first=(address-0x20000000U)/0x1000U,count=size/0x1000U;
    if(uint64_t(first)+count>r.allocated.size())return false;std::fill(r.allocated.begin()+first,r.allocated.begin()+first+count,0);return true;
}
API bool ctr_native_memory_map(void* p,uint32_t base,uint32_t physical,uint32_t size,uint32_t permission,uint32_t state) noexcept {
    try{auto& r=*static_cast<NativeMemory*>(p);++r.calls;
        if(base%0x1000U || physical%0x1000U || size%0x1000U || uint64_t(base)+size>0x100000000ULL || r.stores.ResolveRead(physical,size).size()!=size)return false;
        r.mappings[base]={base,size,physical,permission,state};
        if(r.mappings.size()==1)std::fprintf(stderr,"NATIVE_MEMORY_COUNTERS zakuro_memory_calls=0 native_memory_calls=%llu mappings=1\n",(unsigned long long)r.calls);
        return true;
    }catch(...){return false;}
}
API bool ctr_native_memory_unmap(void* p,uint32_t base,uint32_t size) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;const auto end=uint64_t(base)+size;
    for(auto it=r.mappings.begin();it!=r.mappings.end();){if(it->second.base>=base && uint64_t(it->second.base)+it->second.size<=end)it=r.mappings.erase(it);else ++it;}return true;
}
API bool ctr_native_memory_query(void* p,uint32_t address,Mapping* output) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;if(!output)return false;auto after=r.mappings.upper_bound(address);
    uint32_t start=0;if(after!=r.mappings.begin()){const auto& m=std::prev(after)->second;if(uint64_t(address)<uint64_t(m.base)+m.size){*output=m;return true;}start=m.base+m.size;}
    const uint32_t end=after==r.mappings.end()?0xfffff000U:after->first;*output={start,end-start,0,0,0};return false;
}
API uint32_t ctr_native_memory_snapshot(void* p,Mapping* output,uint32_t capacity) noexcept {
    auto& r=*static_cast<NativeMemory*>(p);++r.calls;if(output){size_t i=0;for(auto& [base,m]:r.mappings){if(i>=capacity)break;output[i++]=m;}}
    return static_cast<uint32_t>(r.mappings.size());
}
