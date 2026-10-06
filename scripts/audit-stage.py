#!/usr/bin/env python3
"""Read-only public-package gate. User-generated game executables stay private."""
import pathlib, subprocess, sys, tomllib, re, hashlib

root=pathlib.Path(sys.argv[1]).resolve()
if not root.is_dir() or not any(root.iterdir()):
    print(f"FAIL Package directory missing or empty: {root}")
    print("STAGE_AUDIT errors=1")
    sys.exit(1)
errors=[]
for path in root.rglob("*"):
    if not path.is_file():continue
    relative=path.relative_to(root)
    if path.name == "Cargo.toml":
        manifest = tomllib.loads(path.read_text())
        build = manifest.get("package", {}).get("build")
        if isinstance(build, str) and not (path.parent / build).is_file():
            errors.append(f"Missing declared Cargo build source: {relative.parent / build}")
    # This pinned Khronos API profile is SDK source, not a user/save profile.
    khronos_profile = relative.as_posix() == "workspace/static-recomp-work/native-renderer/vendor/vulkan_headers/registry/profiles/VP_KHR_roadmap.json" and hashlib.sha256(path.read_bytes()).hexdigest() == "41d8e5b2421bcd78e36d346de18933f5e8ac8e13a82bbcb197bed3fb7fec066d"
    if not khronos_profile and any(part.endswith("-target") or part in {"savedata", "saves", "profiles", "cache", "logs", "target", "source_imports", "source_overlays"} for part in relative.parts):
        errors.append(f"Private data or build cache: {relative}")
    if path.name in {"main", "librecomp.a", "nebby-recomp.json", "library.json"} or path.suffix.lower() in {".keys", ".env"}:
        errors.append(f"Private game/save/config artifact: {relative}")
    if path.suffix.lower() in {".3ds",".cxi",".cci",".cia",".cro",".crs"}:
        errors.append(f"Retail container/module: {relative}")
    if "source_imports" in relative.parts or "source_overlays" in relative.parts:
        errors.append(f"Unrelated reconstructed game source: {relative}")
    with path.open("rb") as stream:
        elf=stream.read(4)==b"\x7fELF"
    if elf or path.suffix.lower() == ".a":
        symbols=subprocess.run(["nm","--defined-only",str(path)],capture_output=True,text=True)
        if any(line.split()[-1:] in (["recomp_entries"],["recomp_entry_count"]) for line in symbols.stdout.splitlines()):
            errors.append(f"Translated retail game executable/library: {relative}")
    if elf:
        dynamic = subprocess.run(["readelf", "-d", str(path)], capture_output=True, text=True)
        for search_path in re.findall(r"(?:RPATH|RUNPATH).*?\[([^\]]*)\]", dynamic.stdout):
            if any(entry.startswith("/") for entry in search_path.split(":")):
                errors.append(f"Non-relative library search path: {relative}")
    if path.suffix.lower() in {".json", ".toml", ".cmake"} or path.name == "CMakeLists.txt":
        if re.search(r"/home/[A-Za-z0-9_.-]+/", path.read_text(errors="replace")):
            errors.append(f"Developer configuration path: {relative}")
for error in errors:print(f"FAIL {error}")
print(f"STAGE_AUDIT errors={len(errors)}")
sys.exit(bool(errors))
