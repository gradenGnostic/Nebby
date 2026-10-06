#!/usr/bin/env python3
"""Mechanical export of generic/native runtime source and local shared bridges.
Never copies a game dump, save, generated recomp archive or game executable.
"""
import pathlib, shutil, sys, json, subprocess

source=pathlib.Path(sys.argv[1]).resolve()
destination=pathlib.Path(sys.argv[2]).resolve()
if not destination.is_dir():raise SystemExit("Create a desktop stage first")
def write_provenance():
    # Logical labels only: no developer paths or private file lists.
    provenance=[]
    for label,relative in (
        ("native-runtime","static-recomp-work/nativeization/remove-zakuro"),
        ("triaevum","tools/external/triaevum"),
        ("3dsrecomp","tools/external/3dsrecomp"),
        ("zakuro-reference","tools/external/zakuro"),
    ):
        repository=source/relative
        commit=subprocess.run(["git","-C",str(repository),"rev-parse","HEAD"],capture_output=True,text=True,check=True).stdout.strip()
        changed=subprocess.run(["git","-C",str(repository),"status","--porcelain","--untracked-files=no"],capture_output=True,text=True,check=True).stdout
        provenance.append({"component":label,"commit":commit,"tracked_local_changes":bool(changed),"scope":"Source origin; exported tree hashes are recorded separately, not a claim of pristine upstream binaries"})
    (destination/"workspace").mkdir(exist_ok=True)
    (destination/"workspace/SOURCE_PROVENANCE.json").write_text(json.dumps(provenance,indent=2)+"\n")
if "--provenance-only" in sys.argv[3:]:
    write_provenance()
    print("SDK_SOURCE_PROVENANCE recorded=4; no source/library replacement")
    sys.exit(0)
skip={".git","target","build","build-core","build-probe","build-bridge","__pycache__","node_modules","evidence","source_imports","source_overlays"}
forbidden={".cxi",".3ds",".cci",".cia",".cro",".crs",".a",".o",".zip",".bin",".pyc"}
private_notes={"FINAL_STATUS.md","SESSION_STATE.md","BASELINE.md","DEBUG_HISTORY.md","MODLOG.md","ACCEPTANCE.md"}
def copy(relative):
    original=source/relative
    if not original.exists():raise SystemExit(f"SDK source missing: {relative}")
    files=[original] if original.is_file() else original.rglob("*")
    for path in files:
        if not path.is_file() or path.is_symlink() or path.name in private_notes:continue
        parts=path.relative_to(original).parts
        if any(part in skip or part.startswith("build-") for part in parts):continue
        # Title gameplay/source-native reconstruction is not a Moon runtime
        # dependency. Never package unrelated retail-derived OoT source.
        if "native_game_runtime" in path.parts and path.name.startswith("oot3d_source_"):continue
        if path.name.startswith("oot3d_source_absolute_data_pilot"):continue
        if path.suffix.lower() in forbidden:continue
        target=destination/"workspace"/path.relative_to(source)
        target.parent.mkdir(parents=True,exist_ok=True)
        shutil.copy2(path,target)

for relative in (
    "static-recomp-work/nativeization/remove-zakuro/runtime",
    "static-recomp-work/nativeization/remove-zakuro/moon-native",
    "static-recomp-work/nativeization/remove-zakuro/titles",
    "static-recomp-work/alpha-sapphire/native",
    "static-recomp-work/alpha-sapphire/profile.json",
    "static-recomp-work/nativeization/remove-zakuro/stage1-services",
    "static-recomp-work/nativeization/stage1-gsp/ctr_native_gsp.cpp",
    "static-recomp-work/nativeization/stage1-gsp/ctr_native_memory.cpp",
    "static-recomp-work/nativeization/stage1-gsp/CMakeLists.txt",
    "static-recomp-work/native-input/overrides",
    "static-recomp-work/single-screen/overrides/native_vector_sync.c",
    "static-recomp-work/native-filesystem/bridge",
    "static-recomp-work/native-filesystem/tests",
    "static-recomp-work/native-filesystem/CMakeLists.txt",
    "static-recomp-work/native-renderer/moon_adapter",
    "static-recomp-work/native-renderer/deps/usr/include/nlohmann",
    "static-recomp-work/native-renderer/deps/usr/share/doc/nlohmann-json3-dev/copyright",
    "tools/external/3dsrecomp/src",
    "tools/external/3dsrecomp/abi",
    "tools/external/3dsrecomp/Cargo.toml",
    "tools/external/3dsrecomp/Cargo.lock",
    "tools/external/3dsrecomp/README.md",
    "tools/external/3dsrecomp/docs",
    "tools/external/triaevum/runtime/three_ds_recomp",
    "tools/external/triaevum/runtime/triaevum_module",
    "tools/external/triaevum/tools/oot3d/native_pica_frontend",
    "tools/external/triaevum/tools/oot3d/source_native_runtime",
    "tools/external/triaevum/tools/oot3d/native_game_runtime",
    "tools/external/triaevum/tools/oot3d/native_a32_runtime/upstream",
    "tools/external/triaevum/tools/ctr_services",
    "tools/external/triaevum/tools/oot3d/third_party/azahar_audio",
    "tools/external/triaevum/tools/oot3d/third_party/azahar_pica",
    "tools/external/triaevum/LICENSE",
    "tools/external/triaevum/LICENSE_SCOPE.md",
    "tools/external/triaevum/LICENSES",
    "tools/external/triaevum/THIRD_PARTY_NOTICES.md",
):copy(relative)

# Binary bridges are generic runtime components, not translated game code.
for relative in (
    "static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so",
    "static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so",
    "static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so",
):
    target=destination/"workspace"/relative
    target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copy2(source/relative,target)

print("Native source/bridge candidate exported; corresponding-source completeness and clean build still require validation.")
recipe=pathlib.Path(__file__).resolve().parent.parent/"native-sdk/CMakeLists.txt"
recipe_target=destination/"workspace/static-recomp-work/native-renderer/runtime-project/CMakeLists.txt"
recipe_target.parent.mkdir(parents=True,exist_ok=True)
shutil.copy2(recipe,recipe_target)

write_provenance()
