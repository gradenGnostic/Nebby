#pragma once
#include "fast/renderer3ds/screen_composition.h"
#include <cstdint>
#include <span>

namespace Fast::Renderer3ds {

// A native primitive group, not a raster crop. Source identifies the group's
// logical UI bounds. The renderer and input adapter consume the same geometry.
struct SingleScreenElement {
    uint64_t SemanticId = 0;
    ScreenRectangle Source;
    ScreenRegion Destination;
    int32_t Layer = 0;
    bool Visible = true;
    bool Interactive = true;
    bool PreserveAspect = true;
};
struct SingleScreenHitRegion {
    uint64_t SemanticId;
    SurfacePoint NativePoint;
};
struct SingleScreenTransform {
    float ScaleX, ScaleY, TranslateX, TranslateY;
    SurfacePoint Apply(SurfacePoint p) const {
        return {p.X*ScaleX+TranslateX,p.Y*ScaleY+TranslateY};
    }
};

inline bool ValidElement(const SingleScreenElement& e) {
    return e.SemanticId!=0 && ValidScreenRegion(e.Destination) &&
        std::isfinite(e.Source.X) && std::isfinite(e.Source.Y) &&
        std::isfinite(e.Source.Width) && std::isfinite(e.Source.Height) &&
        e.Source.Width>0 && e.Source.Height>0;
}
inline ScreenRectangle ElementRectangle(const SingleScreenElement& e,
                                       float width,float height) {
    if (!ValidElement(e)||width<=0||height<=0) return {};
    if (e.PreserveAspect)
        return FitScreenSurface(width,height,e.Source.Width,e.Source.Height,e.Destination);
    return {width*e.Destination.X,height*e.Destination.Y,
            width*e.Destination.Width,height*e.Destination.Height};
}
inline std::optional<SingleScreenTransform> ElementTransform(
    const SingleScreenElement& e,float width,float height) {
    auto r=ElementRectangle(e,width,height);
    if(r.Width<=0||r.Height<=0) return std::nullopt;
    const float sx=r.Width/e.Source.Width,sy=r.Height/e.Source.Height;
    return SingleScreenTransform{sx,sy,r.X-e.Source.X*sx,r.Y-e.Source.Y*sy};
}
// Storage belongs to the title adapter. No discovery, allocation, guest
// pointers, input-library objects, or game-specific policy enters this layer.
struct SingleScreenPage {
    std::span<const SingleScreenElement> Elements;
    std::optional<SingleScreenHitRegion> HitTest(
        float x,float y,float width,float height) const {
        const SingleScreenElement* selected=nullptr;
        SurfacePoint native{};
        for(const auto& e:Elements) {
            if(!e.Visible||!e.Interactive)continue;
            auto p=WindowToSurface(x,y,ElementRectangle(e,width,height),
                                   e.Source.Width,e.Source.Height);
            if(p&&(!selected||e.Layer>=selected->Layer)) {
                selected=&e;native={e.Source.X+p->X,e.Source.Y+p->Y};
            }
        }
        if(!selected)return std::nullopt;
        return SingleScreenHitRegion{selected->SemanticId,native};
    }
};
}
