#!/usr/bin/env python3
"""Export generic renderer dependencies, including local patches and notices."""
import hashlib
import argparse
import json
import pathlib
import shutil
import subprocess
import sys
import re

parser = argparse.ArgumentParser()
parser.add_argument("donor", type=pathlib.Path)
parser.add_argument("stage", type=pathlib.Path)
parser.add_argument("--resume", action="store_true", help="Resume partial export only when existing files match source bytes")
parser.add_argument("--names", nargs="+", choices=("nri", "prism", "imgui", "monocypher", "threadpool", "vma", "vulkan_headers"), default=("nri", "prism", "imgui", "monocypher", "threadpool", "vma", "vulkan_headers"))
args = parser.parse_args()
donor = args.donor.resolve()
stage = args.stage.resolve()
sources = donor / "static-recomp-work/native-renderer/build-probe/_deps"
destination = stage / "workspace/static-recomp-work/native-renderer/vendor"
provenance = destination / "PROVENANCE.json"
records = json.loads(provenance.read_text()) if provenance.is_file() else []
for name in args.names:
    source = sources / f"{name}-src"
    if not source.is_dir():
        raise SystemExit(f"Missing generic dependency: {name}")
    target = destination / name
    if target.exists() and not args.resume:
        raise SystemExit(f"Dependency export refuses overwrite: {name}")
    count = 0
    digest = hashlib.sha256()
    for path in sorted(source.rglob("*")):
        relative = path.relative_to(source)
        if not path.is_file() or path.is_symlink():
            continue
        if any(part in {".git", "build", "target", "__pycache__"} for part in relative.parts[:-1]):
            continue
        if path.suffix.lower() in {".a", ".o", ".so", ".dll", ".exe", ".pdb", ".pyc"}:
            continue
        output = target / relative
        output.parent.mkdir(parents=True, exist_ok=True)
        if output.exists():
            if output.read_bytes() != path.read_bytes():
                raise SystemExit(f"Existing export differs: {name}/{relative}")
        else:
            shutil.copy2(path, output)
        digest.update(relative.as_posix().encode() + b"\0" + hashlib.sha256(path.read_bytes()).digest())
        count += 1
    revision_result = subprocess.run(["git", "-C", str(source), "rev-parse", "HEAD"], capture_output=True, text=True)
    revision = revision_result.stdout.strip() if revision_result.returncode == 0 else None
    status = subprocess.run(["git", "-C", str(source), "status", "--porcelain"], capture_output=True, text=True).stdout.splitlines() if revision else []
    declaration = re.search(r"FetchContent_Declare\(\s*" + re.escape(name) + r"\b(.*?)\n\s*\)", (sources / "nri-src/CMakeLists.txt").read_text(), re.S)
    origin = re.search(r"URL\s+(\S+)", declaration[1]) if declaration else None
    records = [record for record in records if record["name"] != name]
    records.append({"name": name, "commit": revision, "declared_source_url": origin[1] if origin else None, "local_changes": status, "source_files": count, "source_tree_sha256": digest.hexdigest()})
(destination / "PROVENANCE.json").write_text(json.dumps(records, indent=2) + "\n")
print(f"SDK_DEPENDENCIES exported={len(records)}; source and local patches preserved")
