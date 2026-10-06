#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../../.."
task_manifest=static-recomp-work/nativeization/remove-zakuro/moon-native/Cargo.toml
task_audit=static-recomp-work/nativeization/remove-zakuro/moon-native/link-audit.log
cargo tree --offline --target x86_64-unknown-linux-gnu --manifest-path "$task_manifest" > /tmp/moon-native-tree-audit.log
if rg '(^|[[:space:]])zakuro[^[:space:]]* v[0-9]' /tmp/moon-native-tree-audit.log;then exit 1;fi
task_files=(static-recomp-work/native-renderer/moon-target/release/pokemoon-native static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so)
for task_file in "${task_files[@]}";do
 ldd "$task_file" > /tmp/moon-native-ldd-audit.log
 if rg -i 'zakuro|not found' /tmp/moon-native-ldd-audit.log;then exit 1;fi
 nm -C "$task_file" > /tmp/moon-native-symbol-audit.log 2>/dev/null
 if rg 'zakuro(::|_)' /tmp/moon-native-symbol-audit.log;then exit 1;fi
 readelf -d "$task_file" > /tmp/moon-native-dynamic-audit.log
 if rg -i 'NEEDED.*zakuro' /tmp/moon-native-dynamic-audit.log;then exit 1;fi
 sha256sum "$task_file"
done > "$task_audit"
printf '%s\n' 'ZAKURO_LINK_AUDIT pass=yes runtime_libraries=0 runtime_symbols=0' >> "$task_audit"
