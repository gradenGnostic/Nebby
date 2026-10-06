#!/usr/bin/env python3
"""Create a game-free candidate from a private test stage, never publish it.

--hardlink saves disk on a local audit candidate. Do not patch linked files:
recreate the candidate after changes. Default staging copies independently.
"""
import argparse
import os
import pathlib
import shutil
import subprocess
import hashlib

parser = argparse.ArgumentParser()
parser.add_argument("source", type=pathlib.Path)
parser.add_argument("destination", type=pathlib.Path)
parser.add_argument("--hardlink", action="store_true")
args = parser.parse_args()
source = args.source.resolve()
destination = args.destination.resolve()
if not source.is_dir() or destination.exists():
    raise SystemExit("Require existing private stage and NEW candidate directory")
allowed = {"nebby-ui", "LaunchNebby.sh", "tools", "runtimes", "titles", "mods", "licenses", "lib", "workspace", "assets"}
private = {"data", "target", "cache", "profiles", "savedata", "saves", "logs", "evidence", ".git", "source_imports", "source_overlays", "__pycache__"}
forbidden = {".3ds", ".cxi", ".cci", ".cia", ".cro", ".crs", ".a", ".o", ".bin", ".keys", ".pyc"}
bridges = {
    "workspace/static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so",
    "workspace/static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so",
    "workspace/static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so",
}
destination.mkdir()
count = 0
for path in source.rglob("*"):
    relative = path.relative_to(source)
    if relative.parts[0] == "assets" and relative.as_posix() != "assets/nebby.png" and not relative.as_posix().startswith("assets/steamgriddb/"):
        continue
    if relative.parts[0] not in allowed or not path.is_file():
        continue
    if path.is_symlink():
        raise SystemExit(f"Review symlink before packaging: {relative}")
    khronos_profile = relative.as_posix() == "workspace/static-recomp-work/native-renderer/vendor/vulkan_headers/registry/profiles/VP_KHR_roadmap.json" and hashlib.sha256(path.read_bytes()).hexdigest() == "41d8e5b2421bcd78e36d346de18933f5e8ac8e13a82bbcb197bed3fb7fec066d"
    if (not khronos_profile and any(part in private or part.endswith("-target") for part in relative.parts)) or path.suffix.lower() in forbidden:
        continue
    if path.name in {"main", "library.json", "nebby-recomp.json"}:
        continue
    if any(part.startswith("build") for part in relative.parts[:-1]) and relative.as_posix() not in bridges:
        continue
    target = destination / relative
    target.parent.mkdir(parents=True, exist_ok=True)
    if args.hardlink:
        os.link(path, target)
    else:
        shutil.copy2(path, target)
    count += 1
repo = pathlib.Path(__file__).resolve().parent.parent
for name in ["README.md", "LICENSE", "LICENSE.md", "THIRD_PARTY_NOTICES.md"]:
    shutil.copy2(repo / name, destination / name)
result = subprocess.run(["python3", str(repo / "scripts/audit-stage.py"), str(destination)])
print(f"PUBLIC_CANDIDATE files={count} audit_pass={result.returncode == 0}; source/license completeness still requires review")
raise SystemExit(result.returncode)
