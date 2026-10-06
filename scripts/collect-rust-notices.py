#!/usr/bin/env python3
"""Collect Cargo dependency notices without exposing developer source paths.

Run once with all shipped Rust product manifests. Missing notices stay explicit;
this inventory is not a substitute for GPL corresponding-source requirements.
"""
import argparse
import json
import pathlib
import shutil
import subprocess
import re
import urllib.request
import tomllib
import os

parser = argparse.ArgumentParser()
parser.add_argument("stage", type=pathlib.Path)
parser.add_argument("manifests", nargs="+", type=pathlib.Path)
parser.add_argument("--fetch-upstream", action="store_true", help="Fetch missing notices only from exact crate GitHub revision")
parser.add_argument("--lockfile", action="append", type=pathlib.Path, default=[], help="Audit cached registry sources from an existing lockfile without resolving or modifying it")
parser.add_argument("--dep-info-dir", action="append", type=pathlib.Path, default=[], help="Collect registry sources observed in compiled Cargo .d files; conservative build-directory scope, not exact binary linkage")
args = parser.parse_args()
output = args.stage.resolve() / "licenses" / "rust-dependencies"
output.mkdir(parents=True, exist_ok=True)
packages = {}
failed = []
for manifest in args.manifests:
    command = ["cargo", "metadata", "--locked", "--offline", "--filter-platform", "x86_64-unknown-linux-gnu", "--format-version", "1", "--manifest-path", str(manifest.resolve())]
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode:
        failed.append(manifest.parent.name)
        print(f"REVIEW metadata unavailable: {manifest.parent.name}; {result.stderr.splitlines()[0] if result.stderr else 'Cargo failed'}")
        continue
    metadata = json.loads(result.stdout)
    for package in metadata["packages"]:
        package["_workspace_root"] = metadata["workspace_root"]
        packages[(package["name"], package["version"])] = package
for lockfile in args.lockfile:
    registry = pathlib.Path(os.environ.get("CARGO_HOME", pathlib.Path.home() / ".cargo")) / "registry" / "src"
    for package in tomllib.loads(lockfile.read_text())["package"]:
        if not package.get("source", "").startswith("registry+"):
            continue
        name, version = package["name"], package["version"]
        manifests = list(registry.glob(f"*/{name}-{version}/Cargo.toml"))
        if not manifests:
            failed.append(f"{name}-{version}")
            print(f"REVIEW locked crate source unavailable: {name} {version}")
            continue
        manifest = manifests[0]
        declared = tomllib.loads(manifest.read_text())["package"]
        declared.update({"manifest_path": str(manifest), "source": package["source"], "_workspace_root": str(lockfile.parent)})
        packages[(name, version)] = declared
for directory in args.dep_info_dir:
    observed = set()
    for depfile in directory.glob("*.d"):
        # Paths originate in Cargo, not retail resources. Keep host paths out of inventory.
        observed.update(re.findall(r"([^\s:]+/registry/src/[^/\s]+/[^/\s]+)/", depfile.read_text(errors="replace")))
    for source in sorted(observed):
        manifest = pathlib.Path(source) / "Cargo.toml"
        if not manifest.is_file():
            failed.append(pathlib.Path(source).name)
            continue
        declared = tomllib.loads(manifest.read_text())["package"]
        declared.update({"manifest_path": str(manifest), "source": "registry+compiled-dep-info", "_workspace_root": str(manifest.parent)})
        packages[(declared["name"], declared["version"])] = declared
records = []
for (name, version), package in sorted(packages.items()):
    root = pathlib.Path(package["manifest_path"]).parent
    notices = []
    # Local Nebby crates explicitly inherit the repository's MIT license.
    if name.startswith("nebby-") and not package.get("source") and package.get("license") in (None, "MIT"):
        license_path = pathlib.Path(package["_workspace_root"]) / "LICENSE"
        if license_path.is_file():
            notices.append(license_path)
    for path in root.iterdir():
        if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "COPYRIGHT")):
            notices.append(path)
    if package.get("license_file"):
        path = root / package["license_file"]
        if path.is_file() and path not in notices:
            notices.append(path)
    has_code_notice = bool(notices)
    # Font assets have separate licenses; crate MIT/Apache notices do not cover them.
    fonts = root / "fonts"
    if fonts.is_dir():
        notices.extend(path for path in fonts.rglob("*.txt") if path.is_file() and not path.is_symlink())
    target = output / f"{name}-{version}"
    target.mkdir(exist_ok=True)
    names = []
    for path in notices:
        relative = path.relative_to(root) if path.is_relative_to(root) else pathlib.Path(path.name)
        destination = target / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(path, destination)
        names.append(relative.as_posix())
    upstream = []
    if not has_code_notice and args.fetch_upstream:
        vcs = root / ".cargo_vcs_info.json"
        revision = json.loads(vcs.read_text()).get("git", {}).get("sha1", "") if vcs.is_file() else ""
        repository = re.fullmatch(r"https://(github\.com|gitlab\.com)/([\w.-]+)/([\w.-]+?)(?:\.git)?(?:/tree/[^?#]+)?/?", package.get("repository") or "")
        if repository and re.fullmatch(r"[0-9a-f]{40}", revision):
            for filename in ["LICENSE", "LICENSE-MIT", "LICENSE-APACHE", "COPYING"]:
                url = (f"https://raw.githubusercontent.com/{repository[2]}/{repository[3]}/{revision}/{filename}" if repository[1] == "github.com" else f"https://gitlab.com/{repository[2]}/{repository[3]}/-/raw/{revision}/{filename}")
                try:
                    with urllib.request.urlopen(url, timeout=10) as response:
                        content = response.read(200000)
                    if not content or content.lstrip().startswith(b"<!DOCTYPE"):
                        continue
                    (target / filename).write_bytes(content)
                    names.append(filename)
                    upstream.append(url)
                except (OSError, ValueError):
                    pass
    records.append({"name": name, "version": version, "license": package.get("license"), "notices": names, "code_notice_present": has_code_notice or bool(upstream), "upstream_notice_sources": upstream, "source_kind": "registry" if package.get("source", "") and package["source"].startswith("registry+") else "git-or-local"})
(output / "inventory.json").write_text(json.dumps(records, indent=2) + "\n")
(output / "coverage.json").write_text(json.dumps({"manifest_names": [path.parent.name for path in args.manifests], "lockfile_names": [path.parent.name for path in args.lockfile], "compiled_dep_info_directories": len(args.dep_info_dir), "unresolved": failed, "scope": "Dependency/source notice inventory; compiled dep-info includes stale/build-only crates, not proof of exact linked binary versions"}, indent=2) + "\n")
missing = [f"{record['name']} {record['version']}" for record in records if not record["code_notice_present"]]
print(f"RUST_NOTICE_AUDIT packages={len(records)} missing={len(missing)} unresolved_manifests={len(failed)}")
for name in missing:
    print(f"REVIEW {name}")
raise SystemExit(bool(missing or failed))
