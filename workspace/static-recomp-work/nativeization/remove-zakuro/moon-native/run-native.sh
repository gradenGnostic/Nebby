#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
export DISPLAY=${DISPLAY:-:0}
export POKEMOON_RENDERER=triaevum POKEMOON_SINGLE_SCREEN=0 TRIAEVUM_DUAL_SCREEN_SCANOUT=1
export POKEMOON_TRIAEVUM_FRONTEND_LIB="$PWD/static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so"
export POKEMOON_NATIVE_FS_LIB="$PWD/static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so"
export POKEMOON_NATIVE_FS_ROOT="$PWD/static-recomp-work/nativeization/stage0-baseline/test-profile"
export RUST_BACKTRACE=1 RUST_LOG=info
exec "$PWD/static-recomp-work/native-renderer/moon-target/release/pokemoon-native" \
 "$PWD/pokemoon/extracted/cxi/main.fully-decrypted.cxi" \
 "$PWD/static-recomp-work/nativeization/remove-zakuro/titles/moon.json" \
 "$PWD/static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so"
