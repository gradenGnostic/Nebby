// Native CTR offline service contracts. GPL-2.0-or-later.
// Behavioral references: 3dbrew Friend Services and Azahar frd.cpp/ssl_c.cpp.
// No emulator service dispatcher or gameplay state is used here.
#pragma once
#include "oot3d_ctr_ipc_router.h"
#include <array>
#include <cstring>
#include <functional>

namespace Oot3dSourceRuntime {
struct CtrFriendKey {uint32_t PrincipalId=0,Padding=0;uint64_t FriendCode=0;};
struct CtrFriendPresence {
    std::array<uint32_t,6> JoinState{};
    std::array<uint8_t,20> ApplicationArguments{};
    std::array<char16_t,128> GameModeDescription{};
};
static_assert(sizeof(CtrFriendKey)==16&&sizeof(CtrFriendPresence)==0x12c);
struct CtrFriendsProfile {
    CtrFriendKey OwnKey;
    CtrFriendPresence Presence; // Offline: no join availability or game mode.
    std::vector<CtrFriendKey> Friends; // Newly created local account: empty.
};
class CtrFriendsSession final : public CtrIpcSession {
    GuestAddressSpace& mMemory;
    CtrFriendsProfile mProfile;
    GuestAddress mStaticTable{};
    uint32_t mSdkVersion{},mClientProcess{};
    bool Output(std::span<uint32_t> words,std::span<const std::byte> bytes,
                uint32_t header,uint32_t count=0) {
        auto table=mMemory.ResolveRead(mStaticTable,8);
        if(table.size()!=8)return false;
        uint32_t descriptor{},address{};
        std::memcpy(&descriptor,table.data(),4);std::memcpy(&address,table.data()+4,4);
        if((descriptor&0x3fffU)!=2U||(descriptor>>14U)<bytes.size())return false;
        if(!bytes.empty()){
            auto target=mMemory.ResolveWrite(address,bytes.size());
            if(target.size()!=bytes.size())return false;
            std::copy(bytes.begin(),bytes.end(),target.begin());
        }
        words[0]=header;words[1]=0;
        const bool list=(header>>16)==0x11U;
        if(list)words[2]=count;
        words[list?3:2]=(uint32_t(bytes.size())<<14)|2U;
        words[list?4:3]=address;
        return true;
    }
  public:
    CtrFriendsSession(GuestAddressSpace& memory,CtrFriendsProfile profile={})
        :mMemory(memory),mProfile(std::move(profile)){}
    void SetStaticBufferTableAddress(GuestAddress table){mStaticTable=table;}
    CtrResult Dispatch(std::span<uint32_t> words) override {
        if(words.size()<6)return CtrIpcRouter::UnhandledResult;
        switch(words[0]){
        case 0x00320042U:
            if(words[2]!=0x20U)return CtrIpcRouter::UnhandledResult;
            mSdkVersion=words[1];mClientProcess=words[3];
            words[0]=0x00320040U;words[1]=0;return 0;
        case 0x00050000U:
            words[0]=0x00050140U;words[1]=0;
            std::memcpy(words.data()+2,&mProfile.OwnKey,sizeof(mProfile.OwnKey));return 0;
        case 0x00080000U:
            return Output(words,std::as_bytes(std::span(&mProfile.Presence,1)),0x00080042U)?0:CtrIpcRouter::UnhandledResult;
        case 0x00110080U: {
            const auto offset=words[1],capacity=words[2];
            if(capacity>100U)return CtrIpcRouter::UnhandledResult;
            std::vector<CtrFriendKey> records(capacity);
            const auto available=offset<mProfile.Friends.size()?mProfile.Friends.size()-offset:0;
            const auto count=std::min<size_t>(capacity,available);
            if(count)std::copy_n(mProfile.Friends.begin()+offset,count,records.begin());
            return Output(words,std::as_bytes(std::span(records)),0x00110082U,count)?0:CtrIpcRouter::UnhandledResult;
        }
        default:return CtrIpcRouter::UnhandledResult;
        }
    }
};
class CtrSslSession final : public CtrIpcSession {
    GuestAddressSpace& mMemory;
    std::function<bool(std::span<std::byte>)> mRandom;
    bool mInitialized=false;
    uint32_t mProcess{};
  public:
    CtrSslSession(GuestAddressSpace& memory,std::function<bool(std::span<std::byte>)> random)
        :mMemory(memory),mRandom(std::move(random)){}
    CtrResult Dispatch(std::span<uint32_t> words) override {
        if(words.size()<4)return CtrIpcRouter::UnhandledResult;
        if(words[0]==0x00010002U){
            if(words[1]!=0x20U)return CtrIpcRouter::UnhandledResult;
            mProcess=words[2];mInitialized=true;words[0]=0x00010040U;words[1]=0;return 0;
        }
        if(words[0]!=0x00110042U||!mInitialized)return CtrIpcRouter::UnhandledResult;
        const auto count=words[1],descriptor=words[2],address=words[3];
        if(((descriptor&15U)!=8U&&(descriptor&15U)!=12U)||count>(descriptor>>4))
            return CtrIpcRouter::UnhandledResult;
        const auto capacity=descriptor>>4;
        if(capacity){
            auto target=mMemory.ResolveWrite(address,capacity);
            if(target.size()!=capacity||(count&&!mRandom(target.first(count))))return CtrIpcRouter::UnhandledResult;
        }
        words[0]=0x00110042U;words[1]=0;words[2]=descriptor;words[3]=address;
        return 0;
    }
};
class CtrOfflineUdsSession final : public CtrIpcSession {
  public:
    CtrResult Dispatch(std::span<uint32_t> words) override {
        if(words.size()<15||words[0]!=0x001b0302U||words[13]!=0U)
            return CtrIpcRouter::UnhandledResult;
        words[0]=0x001b0040U;words[1]=0xc9411002U; // CTR UDS wireless disabled.
        return 0;
    }
};
}
