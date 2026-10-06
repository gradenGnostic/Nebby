#!/usr/bin/env python3
"""Verify package file hashes. This is not a signature or license check."""
import hashlib
import json
import pathlib
import sys


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(65536), b""):
            result.update(chunk)
    return result.hexdigest()


def verify(root):
    manifest_path = root / "PACKAGE_MANIFEST.json"
    if manifest_path.is_symlink() or manifest_path.stat().st_size > 1048576:
        raise ValueError("Invalid package manifest")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("schema") != 1:
        raise ValueError("Unsupported package manifest schema")
    for artifact in manifest["managed_artifacts"]:
        relative = pathlib.PurePosixPath(artifact["path"])
        if relative.is_absolute() or ".." in relative.parts or not relative.parts:
            raise ValueError("Unsafe artifact path")
        path = root / relative
        if path.is_symlink() or not path.is_file() or path.stat().st_size != artifact["size"] or digest(path) != artifact["sha256"]:
            raise ValueError(f"Changed or missing artifact: {relative}")
    tree = hashlib.sha256()
    count = 0
    for path in sorted(root.rglob("*")):
        if path.is_symlink():
            raise ValueError(f"Unexpected symlink: {path.relative_to(root)}")
        if not path.is_file() or path == manifest_path:
            continue
        tree.update(path.relative_to(root).as_posix().encode() + b"\0" + digest(path).encode() + b"\n")
        count += 1
    if count != manifest["package_files"] or tree.hexdigest() != manifest["package_tree_sha256"]:
        raise ValueError("Package file set or content changed")
    return count


if __name__ == "__main__":
    try:
        count = verify(pathlib.Path(sys.argv[1]).resolve())
    except (ValueError, OSError, KeyError, IndexError) as error:
        raise SystemExit(f"PACKAGE_VERIFY failed: {error}")
    print(f"PACKAGE_VERIFY files={count} pass=yes; unsigned integrity check only")
