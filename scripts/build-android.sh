#!/usr/bin/env bash
set -euo pipefail
nebby_root="$(cd "$(dirname "$0")/.." && pwd)"
moon_root="${POKEMOON_WORKSPACE:-$(dirname "$nebby_root")/pokemonMoondecomp}"
export ANDROID_HOME="${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}"
export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/27.2.12479018}"
export CC_aarch64_linux_android="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android29-clang"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
cmake_bin="${NEBBY_CMAKE:-$moon_root/static-recomp-work/native-renderer/build-tools/bin/cmake}"
gradle_bin="${NEBBY_GRADLE:-gradle}"
build_root="$nebby_root/android/build"
mkdir -p "$build_root/logs" "$build_root/stage/arm64-v8a"
bash "$nebby_root/android/build-recomp.sh" > "$build_root/logs/recomp.log" 2>&1
"$cmake_bin" -S "$nebby_root/android/native" -B "$build_root/native" -G Ninja \
 -DCMAKE_TOOLCHAIN_FILE="$ANDROID_NDK_HOME/build/cmake/android.toolchain.cmake" \
 -DANDROID_ABI=arm64-v8a -DANDROID_PLATFORM=android-29 -DANDROID_STL=c++_shared \
 -DCMAKE_BUILD_TYPE=Release > "$build_root/logs/configure.log" 2>&1
"$cmake_bin" --build "$build_root/native" --target moon_triaevum_frontend_native ctr_native_filesystem \
 -j "${NEBBY_BUILD_JOBS:-2}" > "$build_root/logs/native.log" 2>&1
cargo build --release --manifest-path "$nebby_root/android/runtime/Cargo.toml" \
 --target aarch64-linux-android -j "${NEBBY_BUILD_JOBS:-2}" > "$build_root/logs/rust.log" 2>&1
stage="$build_root/stage/arm64-v8a"
cp "$nebby_root/android/runtime/target/aarch64-linux-android/release/libnebby_android_runtime.so" "$stage/"
cp "$build_root/native/libmoon_triaevum_frontend_native.so" "$stage/"
cp "$build_root/native/filesystem/libctr_native_filesystem.so" "$stage/"
cp "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$stage/"
sdl_library="$(find "$build_root" -path "$build_root/stage" -prune -o -name libSDL2.so -print -quit)"
test -n "$sdl_library";cp "$sdl_library" "$stage/"
# Keep runtime C/JNI exports, but not local debug/symbol tables in the APK.
"$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-strip" --strip-unneeded "$stage"/*.so
sdl_source="$(find "$build_root" -maxdepth 3 -type d -path '*/_deps/sdl2-src' -print -quit)"
test -n "$sdl_source"
"$gradle_bin" -p "$nebby_root/android" --no-daemon --max-workers=2 \
 -PnebbySdlSource="$sdl_source" :app:assembleDebug > "$build_root/logs/apk.log" 2>&1
printf 'APK: %s\n' "$nebby_root/android/app/build/outputs/apk/debug/app-debug.apk"
