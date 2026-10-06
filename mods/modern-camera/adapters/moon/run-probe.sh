#!/usr/bin/env bash
set -euo pipefail
: "${POKEMOON_WORKSPACE:?Set POKEMOON_WORKSPACE to the existing Moon workspace}"
: "${MOON_NATIVE_CTR_LIBRARY:?Set MOON_NATIVE_CTR_LIBRARY to the current native CTR library}"
if pgrep -x pokemoon-native >/dev/null; then
    echo 'Moon already running; refusing a second instance.' >&2
    exit 1
fi
cd "$POKEMOON_WORKSPACE"
export NEBBY_MODERN_CAMERA_TRACE=1
export NEBBY_MODERN_CAMERA_FOLLOW_PROBE="${NEBBY_MODERN_CAMERA_FOLLOW_PROBE:-1}"
# WASD must not also open X/Y actions while walking.
if [[ "$NEBBY_MODERN_CAMERA_FOLLOW_PROBE" == 1 ]]; then
    export NEBBY_KEYBINDS="${NEBBY_KEYBINDS:-circle_up=W;circle_down=S;circle_left=A;circle_right=D;x=C;y=V}"
fi
export POKEMOON_RENDERER=triaevum POKEMOON_SINGLE_SCREEN=0 TRIAEVUM_DUAL_SCREEN_SCANOUT=1
export POKEMOON_TRIAEVUM_FRONTEND_LIB="$PWD/static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so"
export POKEMOON_NATIVE_FS_LIB="$PWD/static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so"
export POKEMOON_NATIVE_FS_ROOT="${POKEMOON_NATIVE_FS_ROOT:-$PWD/static-recomp-work/single-screen/test-profile}"
export RUST_LOG=warn
exec "$PWD/static-recomp-work/native-renderer/moon-target/release/pokemoon-native" \
    "$PWD/pokemoon/extracted/cxi/main.fully-decrypted.cxi" \
    "$PWD/static-recomp-work/nativeization/remove-zakuro/titles/moon.json" \
    "$MOON_NATIVE_CTR_LIBRARY"
