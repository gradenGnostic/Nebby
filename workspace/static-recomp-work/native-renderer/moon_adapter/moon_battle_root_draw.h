#pragma once
#include "moon_battle_root.h"
#include "imgui.h"
namespace MoonBattleRoot {
// Render only a verified semantic root snapshot. The caller owns guest
// lifecycle and dispatch; this function cannot open any game menu itself.
inline void Draw(ImDrawList& draw,float width,float height,const CommandMap& map,
                 int nativeSelected,std::optional<Action> hover={}){
    if(!map.Verified())return;
    const auto selected=map.ActionFor(nativeSelected);
    const auto rects=Layout(width,height);
    for(unsigned i=0;i<4;i++){
        const auto r=rects[i];const auto rgba=Colors[i];
        const auto color=IM_COL32((rgba>>24)&255,(rgba>>16)&255,(rgba>>8)&255,235);
        const ImVec2 start(r.x,r.y),end(r.x+r.w,r.y+r.h);
        draw.AddRectFilled(start,end,color,5.f);
        if(selected==static_cast<Action>(i))
            draw.AddRect(start,end,IM_COL32(255,245,190,255),5.f,0,3.f);
        else if(hover==static_cast<Action>(i))
            draw.AddRect(start,end,IM_COL32(235,235,235,255),5.f,0,1.f);
        const auto size=ImGui::CalcTextSize(Labels[i]);
        draw.AddText(ImVec2(r.x+(r.w-size.x)/2,r.y+(r.h-size.y)/2),
                     IM_COL32(255,255,255,255),Labels[i]);
    }
}
}
