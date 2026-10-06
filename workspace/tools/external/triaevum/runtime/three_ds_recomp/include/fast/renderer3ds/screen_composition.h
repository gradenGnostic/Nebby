#pragma once
#include <algorithm>
#include <cmath>
#include <optional>

namespace Fast::Renderer3ds {
// Title-neutral surface region. Units are fractions of the final drawable.
struct ScreenRegion { float X=0, Y=0, Width=1, Height=1; };
struct ScreenRectangle { float X=0,Y=0,Width=0,Height=0; };
inline bool ValidScreenRegion(const ScreenRegion& r) {
    return std::isfinite(r.X)&&std::isfinite(r.Y)&&std::isfinite(r.Width)&&std::isfinite(r.Height)&&r.X>=0&&r.Y>=0&&r.Width>0&&r.Height>0&&r.X+r.Width<=1.00001f&&r.Y+r.Height<=1.00001f;
}
inline ScreenRectangle FitScreenSurface(float width,float height,float sourceWidth,float sourceHeight,ScreenRegion region={}) {
    if(!ValidScreenRegion(region)||width<=0||height<=0||sourceWidth<=0||sourceHeight<=0)return {};
    const float scale=std::min(width*region.Width/sourceWidth,height*region.Height/sourceHeight);
    const float w=sourceWidth*scale,h=sourceHeight*scale;
    return {width*region.X+(width*region.Width-w)*0.5f,height*region.Y+(height*region.Height-h)*0.5f,w,h};
}
struct SurfacePoint { float X,Y; };
inline std::optional<SurfacePoint> WindowToSurface(float x,float y,const ScreenRectangle&r,float sourceWidth,float sourceHeight){
    if(r.Width<=0||r.Height<=0||x<r.X||y<r.Y||x>=r.X+r.Width||y>=r.Y+r.Height)return std::nullopt;
    return SurfacePoint{(x-r.X)*sourceWidth/r.Width,(y-r.Y)*sourceHeight/r.Height};
}
inline SurfacePoint SurfaceToWindow(float x,float y,const ScreenRectangle&r,float sourceWidth,float sourceHeight){return {r.X+x*r.Width/sourceWidth,r.Y+y*r.Height/sourceHeight};}
}
