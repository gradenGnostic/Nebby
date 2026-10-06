#pragma once
#include <array>
#include <cstdint>
#include <optional>

// Moon-specific presentation only. Numeric native IDs must be populated from
// runtime evidence, never inferred from the visual order of retail buttons.
namespace MoonBattleRoot {
enum Action : unsigned { Fight, Bag, Pokemon, Run };
struct Rect {
    float x,y,w,h;
    bool Contains(float px,float py) const {
        return px>=x&&py>=y&&px<x+w&&py<y+h;
    }
};
inline constexpr std::array<const char*,4> Labels{"Fight","Bag","Pokémon","Run"};
inline constexpr std::array<uint32_t,4> Colors{0xb63b48ff,0x356ab4ff,0x38985bff,0x7955acff};
struct CommandMap {
    std::array<int,4> ids{-1,-1,-1,-1};
    bool Verified() const {
        unsigned seen=0;
        for(int id:ids){if(id<0||id>3||(seen&(1u<<id)))return false;seen|=1u<<id;}
        return seen==15;
    }
    std::optional<Action> ActionFor(int nativeId) const {
        if(!Verified())return {};
        for(unsigned i=0;i<4;i++)if(ids[i]==nativeId)return static_cast<Action>(i);
        return {};
    }
};
// One geometry source for rendering, hover and clicks, normalized to window.
inline std::array<Rect,4> Layout(float width,float height){
    if(width<=0||height<=0)return {};
    const float gap=width*.008f,w=width*.135f,h=height*.075f;
    const float x=width*.98f-2*w-gap,y=height*.96f-2*h-gap;
    return {{{x,y,w,h},{x+w+gap,y,w,h},{x,y+h+gap,w,h},{x+w+gap,y+h+gap,w,h}}};
}
inline std::optional<Action> Hit(float width,float height,float x,float y){
    const auto rects=Layout(width,height);
    for(unsigned i=0;i<4;i++)if(rects[i].Contains(x,y))return static_cast<Action>(i);
    return {};
}
inline Action Navigate(Action selected,int dx,int dy){
    unsigned row=unsigned(selected)/2,col=unsigned(selected)%2;
    if(dx<0)col=0;else if(dx>0)col=1;
    if(dy<0)row=0;else if(dy>0)row=1;
    return static_cast<Action>(row*2+col);
}
}
