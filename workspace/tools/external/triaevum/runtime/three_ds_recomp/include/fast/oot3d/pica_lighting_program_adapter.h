#pragma once

#include "fast/oot3d/pica_fragment_lighting.h"
#include "fast/renderer3ds/pica_lighting_program.h"

namespace Fast::Renderer3ds {

inline PicaLightingProgram BuildPicaLightingProgram(const Oot3d::PicaFragmentLightingState& state) {
    PicaLightingProgram p;
    p.Control = {state.ActiveLightCount, state.EnvironmentConfiguration, state.FresnelSelector,
        uint32_t(state.ClampHighlights) | (uint32_t(state.ShadowPrimary) << 1) |
        (uint32_t(state.ShadowSecondary) << 2) | (uint32_t(state.ShadowAlpha) << 3) |
        (uint32_t(state.BumpMode) << 4) | (uint32_t(state.RecalculateBumpVectors) << 6) |
        (uint32_t(state.ShadowFactorEnabled) << 7) | (uint32_t(state.InvertShadow) << 8) |
        (uint32_t(state.BumpTextureUnit) << 9) | (uint32_t(state.ShadowTextureUnit) << 11)};
    for (size_t slot = 0; slot < p.Lights.size(); ++slot) {
        const auto index = state.LightPermutation[slot];
        const auto& light = state.Lights[index];
        const bool spot = light.SpotAttenuationEnabled &&
            Oot3d::IsPicaLightingLutSamplerSupported(state.EnvironmentConfiguration, uint8_t(8 + index));
        p.Lights[slot] = {index, uint32_t(light.Directional) | (uint32_t(light.TwoSidedDiffuse) << 1) |
            (uint32_t(light.GeometricFactor0) << 2) | (uint32_t(light.GeometricFactor1) << 3) |
            (uint32_t(light.ShadowEnabled) << 4) | (uint32_t(spot) << 5) |
            (uint32_t(light.DistanceAttenuationEnabled) << 6), 0, 0};
    }
    constexpr uint8_t tables[]{0, 1, 8, 3, 4, 5, 6};
    constexpr uint8_t disableBits[]{16, 17, 0, 19, 22, 21, 20};
    for (size_t i = 0; i < p.Luts.size(); ++i) {
        const auto& lut = state.LutSamplers[i];
        const bool enabled = i == 2 || ((state.Config1 & (1U << disableBits[i])) == 0 &&
            Oot3d::IsPicaLightingLutSamplerSupported(state.EnvironmentConfiguration, tables[i]));
        p.Luts[i] = {float(lut.Input), float(lut.AbsoluteInput), lut.Scale, float(enabled)};
    }
    return p;
}

} // namespace Fast::Renderer3ds
