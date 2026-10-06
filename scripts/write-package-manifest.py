#!/usr/bin/env python3
"""Fingerprint managed tools/native SDK without recording user data or paths."""
import hashlib
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(sys.argv[1]).resolve()
audit = pathlib.Path(__file__).with_name("audit-stage.py")
subprocess.run(["python3", str(audit), str(root)], check=True)

def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            result.update(chunk)
    return result.hexdigest()

components = [
    "nebby-ui", "tools/3dsrecomp/3dsrecomp", "runtimes/zakuro/zakuro",
    "workspace/static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so",
    "workspace/static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so",
    "workspace/static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so",
]
artifacts = []
for relative in components:
    path = root / relative
    if not path.is_file():
        raise SystemExit(f"Required managed artifact missing: {relative}")
    artifacts.append({"path": relative, "size": path.stat().st_size, "sha256": digest(path)})

sources = hashlib.sha256()
count = 0
for path in sorted((root / "workspace").rglob("*")):
    if not path.is_file() or path.suffix.lower() in {".so", ".a", ".o"}:
        continue
    relative = path.relative_to(root).as_posix()
    sources.update(relative.encode() + b"\0" + digest(path).encode() + b"\n")
    count += 1
manifest = {
    "schema": 1,
    "distribution_status": "candidate_only_license_review_pending",
    "retail_game_data_included": False,
    "native_moon_executable_included": False,
    "managed_artifacts": artifacts,
    "sdk_source_files": count,
    "sdk_source_tree_sha256": sources.hexdigest(),
    "source_hash_scope": "Exported workspace files excluding compiled libraries/objects",
}
license_tree = hashlib.sha256()
license_count = 0
for path in sorted((root / "licenses").rglob("*")):
    if path.is_file():
        license_tree.update(path.relative_to(root).as_posix().encode() + b"\0" + digest(path).encode() + b"\n")
        license_count += 1
manifest["license_and_source_notice_files"] = license_count
manifest["license_and_source_notice_tree_sha256"] = license_tree.hexdigest()
package_tree = hashlib.sha256()
package_files = 0
for path in sorted(root.rglob("*")):
    if not path.is_file() or path == root / "PACKAGE_MANIFEST.json":
        continue
    if path.is_symlink():
        raise SystemExit(f"Review symlink before packaging: {path.relative_to(root)}")
    package_tree.update(path.relative_to(root).as_posix().encode() + b"\0" + digest(path).encode() + b"\n")
    package_files += 1
manifest["package_files"] = package_files
manifest["package_tree_sha256"] = package_tree.hexdigest()
manifest["package_hash_scope"] = "All regular package files except PACKAGE_MANIFEST.json"
(root / "PACKAGE_MANIFEST.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(f"PACKAGE_MANIFEST artifacts={len(artifacts)} source_files={count}; license approval not implied")
