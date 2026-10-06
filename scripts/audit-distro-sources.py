#!/usr/bin/env python3
"""Check exact distro source archive coverage; not a license/legal clearance."""
import hashlib
import json
import pathlib
import re
import sys

root = pathlib.Path(sys.argv[1]).resolve()
directory = root / "licenses/corresponding-sources"
inventory = json.loads((root / "licenses/desktop-dependencies/inventory.json").read_text())
sources = json.loads((directory / "SOURCE_ARCHIVES.json").read_text())
errors = []
verified = set()
def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        while chunk := stream.read(65536):
            result.update(chunk)
    return result.hexdigest()
for source in sources:
    name, version = source["source_package"], source["source_version"]
    if not re.fullmatch(r"[a-z0-9][a-z0-9+.-]*", name) or not re.fullmatch(r"[0-9][A-Za-z0-9.+:~_-]*", version):
        errors.append("Unsafe source identity")
        continue
    folder = directory / f"{name}-{version.replace(':', '_')}"
    files = [(source["descriptor"], source["descriptor_sha256"], None)]
    files += [(archive["file"], archive["sha256"], archive["size"]) for archive in source["archives"]]
    valid = bool(source["archives"])
    for filename, expected, size in files:
        if pathlib.PurePosixPath(filename).name != filename:
            valid = False
            continue
        path = folder / filename
        if not path.is_file() or path.is_symlink() or digest(path) != expected or (size is not None and path.stat().st_size != size):
            valid = False
    if valid:
        verified.add((name, version))
    else:
        errors.append(f"Incomplete/changed source archive set: {name} {version}")
needed = set()
matched_libraries = 0
for library in inventory:
    origins = [source for source in library["installed_source_metadata"] if source["staged_build_id_matches"]]
    if not origins:
        errors.append(f"Missing verified binary origin: {library['library']}")
        continue
    matched_libraries += 1
    needed.update((source["source_package"], source["source_version"]) for source in origins)
for name, version in sorted(needed - verified):
    errors.append(f"Missing exact source: {name} {version}")
for error in errors:
    print(f"FAIL {error}")
print(f"DISTRO_SOURCE_AUDIT libraries={matched_libraries}/{len(inventory)} source_sets={len(needed & verified)}/{len(needed)} errors={len(errors)}; not legal clearance")
sys.exit(bool(errors))
