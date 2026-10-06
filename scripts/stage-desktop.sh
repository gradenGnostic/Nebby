#!/usr/bin/env bash
# Stage managed launcher/tools without retail content. Native SDK source export
# and donor-source/license gates must be completed before public release.
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
task_workspace="${POKEMOON_WORKSPACE:-$task_root/workspace}"
task_recompiler="$task_root/tools/3dsrecomp/3dsrecomp"
task_zakuro="$task_root/runtimes/zakuro/zakuro"
if [ ! -f "$task_recompiler" ]; then
    task_recompiler="$task_workspace/tools/external/3dsrecomp/target/release/3dsrecomp"
fi
if [ ! -f "$task_zakuro" ]; then
    task_zakuro="$task_workspace/tools/external/zakuro/target/release/zakuro"
fi
task_output="${1:?usage: stage-desktop.sh NEW-output-directory}"
if [ -e "$task_output" ]; then
    echo "Refusing to overwrite existing staging directory" >&2
    exit 1
fi
for task_binary in "$task_root/target/release/nebby-ui" "$task_recompiler" "$task_zakuro"; do
    test -f "$task_binary" || { echo "Required build missing: $task_binary" >&2; exit 1; }
done
mkdir -p "$task_output/tools/3dsrecomp" "$task_output/runtimes/zakuro" "$task_output/titles" "$task_output/mods" "$task_output/licenses" "$task_output/workspace"
cp "$task_root/target/release/nebby-ui" "$task_output/nebby-ui"
cp "$task_recompiler" "$task_output/tools/3dsrecomp/3dsrecomp"
cp "$task_zakuro" "$task_output/runtimes/zakuro/zakuro"
cp "$task_root"/titles/*.json "$task_output/titles/"
cp "$task_root/mods/catalog.json" "$task_output/mods/catalog.json"
cp -R "$task_root/licenses/." "$task_output/licenses/"
cp "$task_root/LICENSE" "$task_root/THIRD_PARTY_NOTICES.md" "$task_output/licenses/"
cp "$task_root/scripts/LaunchNebby.sh" "$task_output/LaunchNebby.sh"
# Bundled offline artwork and source/author provenance.
if [ -d "$task_root/assets/steamgriddb" ]; then
    mkdir -p "$task_output/assets"
    cp -R "$task_root/assets/steamgriddb" "$task_output/assets/"
fi
if [ -f "$task_root/assets/nebby.png" ]; then
    mkdir -p "$task_output/assets"
    cp "$task_root/assets/nebby.png" "$task_output/assets/"
fi
chmod +x "$task_output/LaunchNebby.sh"
echo "Staged launcher and managed tools. Native SDK and donor-source export are still required."
echo "No ROM, save, keys or generated game executable was copied."
