#!/usr/bin/env python3
"""Collect existing distro notices; unresolved matches remain release blockers."""
import pathlib, shutil, subprocess, sys, json, re, hashlib, io, tarfile, tempfile
def build_id(path):
    result=subprocess.run(["readelf","-n",str(path)],capture_output=True,text=True)
    match=re.search(r"Build ID: ([0-9a-f]+)",result.stdout)
    return match.group(1) if match else None
root=pathlib.Path(sys.argv[1]).resolve()
donor=pathlib.Path(sys.argv[2]).resolve()
out=root/"licenses"/"desktop-dependencies"
out.mkdir(parents=True,exist_ok=True)
records=[]
for library in sorted((root/"lib").iterdir()):
    candidates=[]
    source_records=[]
    staged_id=build_id(library)
    for directory in (pathlib.Path("/lib/x86_64-linux-gnu"),pathlib.Path("/usr/lib/x86_64-linux-gnu"),pathlib.Path("/usr/lib/x86_64-linux-gnu/pulseaudio")):
        path=directory/library.name
        if path.exists():
            query=subprocess.run(["dpkg-query","-S",str(path.resolve())],capture_output=True,text=True)
            for line in query.stdout.splitlines():
                package=line.split(": ",1)[0].split(":",1)[0]
                notice=pathlib.Path("/usr/share/doc")/package/"copyright"
                if notice.is_file():
                    candidates.append((package,notice))
                    metadata=subprocess.run(["dpkg-query","-W","-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}\n",package+":amd64"],capture_output=True,text=True)
                    fields=metadata.stdout.strip().split("\t")
                    if metadata.returncode==0 and len(fields)==4:
                        source_records.append({"binary_package":fields[0],"binary_version":fields[1],"source_package":fields[2],"source_version":fields[3],"installed_build_id":build_id(path),"staged_build_id_matches":bool(staged_id and staged_id==build_id(path)),"corresponding_source_supplied":False})
    prefix=donor/"static-recomp-work/native-renderer/deps/usr/share/doc"
    if not candidates and prefix.is_dir():
        # Explicit donor dependency names only, never guess license ownership.
        stems={"libzip.so.4":"libzip4","libtinyxml2.so.10":"libtinyxml2-10","libspdlog.so.1.12":"libspdlog1.12","libfmt.so.9":"libfmt9","libshaderc.so.1":"libshaderc1"}
        package=stems.get(library.name)
        if package and (prefix/package/"copyright").is_file():
            candidates.append((package,prefix/package/"copyright"))
            for archive in sorted((donor/"static-recomp-work/native-renderer/deps").rglob("*.deb")):
                info=subprocess.run(["dpkg-deb","-f",str(archive),"Package","Version","Source"],capture_output=True,text=True)
                fields=dict(line.split(": ",1) for line in info.stdout.splitlines() if ": " in line)
                if fields.get("Package")!=package:continue
                payload=subprocess.run(["dpkg-deb","--fsys-tarfile",str(archive)],capture_output=True,check=True).stdout
                with tarfile.open(fileobj=io.BytesIO(payload)) as contents:
                    matches=[entry for entry in contents.getmembers() if entry.isfile() and pathlib.PurePosixPath(entry.name).name.startswith(library.name)]
                    if len(matches)!=1:continue
                    with tempfile.NamedTemporaryFile() as extracted:
                        extracted.write(contents.extractfile(matches[0]).read())
                        extracted.flush()
                        archive_id=build_id(extracted.name)
                source=fields.get("Source",package)
                source_match=re.fullmatch(r"([^ ]+)(?: \(([^)]+)\))?",source)
                source_records.append({"binary_package":package,"binary_version":fields.get("Version"),"source_package":source_match[1] if source_match else source,"source_version":(source_match[2] if source_match else None) or fields.get("Version"),"deb_sha256":hashlib.sha256(archive.read_bytes()).hexdigest(),"archive_build_id":archive_id,"staged_build_id_matches":bool(staged_id and staged_id==archive_id),"corresponding_source_supplied":False})
    packages=[]
    for package,notice in candidates:
        target=out/package
        target.mkdir(exist_ok=True)
        shutil.copy2(notice,target/"copyright")
        # Distro copyright files often reference, rather than contain, full terms.
        referenced=set(re.findall(r"/usr/share/common-licenses/([A-Za-z0-9.+-]+)",notice.read_text(errors="replace")))
        for name in sorted(referenced):
            license_file=pathlib.Path("/usr/share/common-licenses")/name.rstrip(".")
            if license_file.is_file():
                shutil.copy2(license_file,target/license_file.name)
        packages.append(package)
    records.append({"library":library.name,"sha256":hashlib.sha256(library.read_bytes()).hexdigest(),"build_id":staged_id,"notice_packages":sorted(set(packages)),"installed_source_metadata":source_records})
(out/"inventory.json").write_text(json.dumps(records,indent=2)+"\n")
missing=[r["library"] for r in records if not r["notice_packages"]]
print(f"Dependency notices: matched={len(records)-len(missing)} unresolved={len(missing)}")
if missing:print("Unresolved: "+", ".join(missing))
