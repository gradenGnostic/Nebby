#!/usr/bin/env bash
set -euo pipefail
task_bundle="$(cd "$(dirname "$0")" && pwd)"
if [[ "$(uname -s)" != Linux || "$(uname -m)" != x86_64 ]]; then
    echo "This bundle requires Linux x86_64. Build Nebby from source for other platforms." >&2
    exit 1
fi
task_libc="$(getconf GNU_LIBC_VERSION 2>/dev/null || true)"
if [[ ! "$task_libc" =~ ^glibc\ ([0-9]+)\.([0-9]+)$ ]] ||
   (( BASH_REMATCH[1] < 2 || (BASH_REMATCH[1] == 2 && BASH_REMATCH[2] < 39) )); then
    echo "This bundle requires glibc 2.39 or newer. Use a newer distribution or build from source." >&2
    exit 1
fi
for task_required in nebby-ui tools/3dsrecomp/3dsrecomp runtimes/zakuro/zakuro; do
    if [[ ! -x "$task_bundle/$task_required" ]]; then
        echo "Missing executable: $task_required. Extract the complete bundle and preserve permissions." >&2
        exit 1
    fi
done
export NEBBY_APP_ROOT="$task_bundle"
export NEBBY_DATA_DIR="${NEBBY_DATA_DIR:-$task_bundle/data}"
export POKEMOON_WORKSPACE="${POKEMOON_WORKSPACE:-$task_bundle/workspace}"
export LD_LIBRARY_PATH="$task_bundle/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cd "$task_bundle"
exec "$task_bundle/nebby-ui" "$@"
