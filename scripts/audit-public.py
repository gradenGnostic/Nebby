#!/usr/bin/env python3
"""Read-only tracked-source release gate; never scans private user data."""
import pathlib, re, subprocess, sys, argparse
parser=argparse.ArgumentParser()
parser.add_argument("--repo",type=pathlib.Path,default=pathlib.Path(__file__).resolve().parent.parent)
parser.add_argument("--current-tree-only",action="store_true",help="Audit current source snapshot, not GitHub history readiness")
args=parser.parse_args()
root=args.repo.resolve()
tracked=subprocess.check_output(["git","-C",str(root),"ls-files","-z"]).decode().split("\0")
problems=[]
generic_libraries={
    "lib/libpulsecommon-16.1.so",
    "workspace/static-recomp-work/native-filesystem/build-bridge/libctr_native_filesystem.so",
    "workspace/static-recomp-work/native-renderer/build-probe/libmoon_triaevum_frontend_native.so",
    "workspace/static-recomp-work/nativeization/stage1-gsp/build/libctr_native_gsp.so",
}
for name in filter(None,tracked):
    path=root/name
    if path.suffix.lower() in {".cxi",".3ds",".cci",".cia",".zip",".keys",".a"} or (path.suffix.lower()==".so" and name not in generic_libraries):
        problems.append(f"Forbidden public artifact: {name}")
    if path.suffix.lower()==".so" and name in generic_libraries and path.is_file():
        with path.open("rb") as stream:
            elf=stream.read(4)==b"\x7fELF"
        if not elf:
            problems.append(f"Invalid generic library: {name}")
        else:
            for options in (["--defined-only"],["-D","--defined-only"]):
                symbols=subprocess.run(["nm",*options,str(path)],capture_output=True,text=True)
                if symbols.returncode:
                    problems.append(f"Cannot audit generic library symbols: {name}")
                    break
                if re.search(r"\b(recomp_entries|recomp_entry_count)\s*$",symbols.stdout,re.MULTILINE):
                    problems.append(f"Translated retail code in generic library: {name}")
                    break
    if path.is_file() and path.stat().st_size<2_000_000:
        text=path.read_bytes().decode("utf-8",errors="ignore")
        if re.search(r"/home/[A-Za-z0-9_.-]+/",text):
            problems.append(f"Developer absolute path: {name}")
for name in ("README.md","LICENSE","THIRD_PARTY_NOTICES.md"):
    if not(root/name).is_file():problems.append(f"Missing release document: {name}")
if "assets/nebby.png" in tracked or "assets/nebby-original.jpg" in tracked:
    problems.append("Legacy branding rights unresolved: verify permission or replace assets before public release")
if any(name.startswith("assets/steamgriddb/") for name in tracked):
    problems.append("Personal-server SteamGridDB artwork: obtain permission or exclude before public release")
if not args.current_tree_only:
    # Object names are deduplicated by blob hash and can hide a second filename.
    history=subprocess.run(["git","-C",str(root),"log","--format=","--name-only","--no-renames","-z","HEAD"],capture_output=True,text=True,check=True)
    for name in sorted(set(path.strip("\n") for path in history.stdout.split("\0") if path.strip("\n"))):
        if name in {"assets/nebby.png","assets/nebby-original.jpg"}:
            problems.append(f"Unlicensed legacy branding in reachable Git history: {name}")
        elif pathlib.PurePosixPath(name).suffix.lower() in {".cxi",".3ds",".cci",".cia",".zip",".keys",".a"} or (pathlib.PurePosixPath(name).suffix.lower()==".so" and name not in generic_libraries):
            problems.append(f"Forbidden artifact in reachable Git history: {name}")
for problem in problems:print(problem)
print(f"PUBLIC_AUDIT scope={'current-tree' if args.current_tree_only else 'current-tree-and-history'} errors={len(problems)}")
sys.exit(bool(problems))
