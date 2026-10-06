#!/usr/bin/env python3
"""Collect ELF dependencies into a candidate stage; patch staged copies only."""
import pathlib, re, shutil, subprocess, sys, os
root=pathlib.Path(sys.argv[1]).resolve()
patcher=sys.argv[2]
lib=root/"lib"
lib.mkdir(exist_ok=True)
binaries=[root/"nebby-ui",root/"tools/3dsrecomp/3dsrecomp",root/"runtimes/zakuro/zakuro"]
binaries+=list((root/"workspace").rglob("*.so"))
for binary in binaries+list(lib.iterdir()):
    if binary.stat().st_nlink > 1:
        raise SystemExit(f"Refusing to patch shared hardlink: {binary.relative_to(root)}; use an independent copied stage")
dependencies={}
for binary in binaries:
    text=subprocess.check_output(["ldd",str(binary)],text=True)
    if "not found" in text:raise SystemExit(f"Missing dependency for {binary.name}")
    for name,path in re.findall(r"\s+(\S+) => (/\S+)",text):
        if name in {"libc.so.6","libm.so.6","libpthread.so.0","libdl.so.2","librt.so.1"}:continue
        dependencies[name]=path
for name,path in dependencies.items():
    destination=lib/name
    if not destination.exists():shutil.copy2(path,destination)
for binary in binaries+list(lib.iterdir()):
    relative=os.path.relpath(lib,binary.parent)
    subprocess.run([patcher,"--set-rpath",f"$ORIGIN/{relative}",str(binary)],check=True)
print(f"Staged {len(dependencies)} shared-library dependencies; physical GPU drivers remain host supplied.")
print("Audit dependency licenses/corresponding source before public release.")
