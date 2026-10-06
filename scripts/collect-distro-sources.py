#!/usr/bin/env python3
"""Retrieve exact Ubuntu HTTPS source packages; verify archive hashes.

This collects source, not legal clearance or proof of a reproducible binary build.
"""
import argparse
import hashlib
import json
import pathlib
import re
import subprocess
import urllib.request
import urllib.parse
import urllib.error

parser = argparse.ArgumentParser()
parser.add_argument("inventory", type=pathlib.Path)
parser.add_argument("destination", type=pathlib.Path)
parser.add_argument("--package", action="append", default=[])
parser.add_argument("--all-verified", action="store_true", help="Conservatively collect every verified binary package source, irrespective of license")
parser.add_argument("--keyring", default="/usr/share/keyrings/ubuntu-archive-keyring.gpg")
parser.add_argument("--require-signature", action="store_true", help="Require uploader signature validation against supplied keyring")
args = parser.parse_args()
packages = {}
for library in json.loads(args.inventory.read_text()):
    for source in library.get("installed_source_metadata", []):
        if (args.all_verified or source["source_package"] in args.package) and source["staged_build_id_matches"]:
            packages[(source["source_package"], source["source_version"])] = True
if (not args.all_verified and set(args.package) != {name for name, version in packages}) or not packages:
    raise SystemExit("Selected package lacks verified staged-binary source metadata")
args.destination.mkdir(parents=True, exist_ok=True)
record_path = args.destination / "SOURCE_ARCHIVES.json"
records = json.loads(record_path.read_text()) if record_path.is_file() else []
def download(url, target, expected=None):
    if target.exists():
        if expected and hashlib.sha256(target.read_bytes()).hexdigest() == expected:
            return
        raise RuntimeError(f"Refusing unchecked overwrite: {target.name}")
    pending = target.with_suffix(target.suffix + ".pending")
    if pending.exists():
        raise RuntimeError(f"Review interrupted source download: {pending.name}")
    with urllib.request.urlopen(url, timeout=30) as response, pending.open("xb") as stream:
        total = 0
        while chunk := response.read(65536):
            total += len(chunk)
            if total > 256 * 1024 * 1024:
                raise RuntimeError("Source archive exceeds bounded download limit")
            stream.write(chunk)
    if expected and hashlib.sha256(pending.read_bytes()).hexdigest() != expected:
        raise RuntimeError(f"Source hash mismatch: {target.name}")
    pending.rename(target)
for name, version in sorted(packages):
    if not re.fullmatch(r"[a-z0-9][a-z0-9+.-]*", name):
        raise RuntimeError("Invalid source package name")
    if not re.fullmatch(r"[0-9][A-Za-z0-9.+:~_-]*", version):
        raise RuntimeError("Invalid source package version")
    directory = args.destination / f"{name}-{version.replace(':', '_')}"
    directory.mkdir(exist_ok=True)
    filename = f"{name}_{version.split(':')[-1]}.dsc"
    prefix = name[:4] if name.startswith("lib") else name[0]
    descriptor = directory / filename
    base = None
    for component in ("main", "universe", "restricted", "multiverse"):
        candidate = f"https://archive.ubuntu.com/ubuntu/pool/{component}/{prefix}/{name}/"
        try:
            if descriptor.exists():
                with urllib.request.urlopen(candidate + urllib.parse.quote(filename), timeout=30) as response:
                    remote = response.read(1048576)
                if descriptor.read_bytes() != remote:
                    raise RuntimeError("Cached source descriptor differs from official archive")
            else:
                download(candidate + urllib.parse.quote(filename), descriptor)
            base = candidate
            break
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
    if base is None:
        # Superseded versions leave the live pool but remain in Ubuntu's archive.
        query = urllib.parse.urlencode({"ws.op": "getPublishedSources", "source_name": name, "version": version, "exact_match": "true"})
        with urllib.request.urlopen("https://api.launchpad.net/1.0/ubuntu/+archive/primary?" + query, timeout=30) as response:
            publications = json.load(response)
        for publication in publications.get("entries", []):
            publication_url = publication.get("self_link", "")
            if not re.fullmatch(r"https://api\.launchpad\.net/1\.0/ubuntu/\+archive/primary/\+sourcepub/[0-9]+", publication_url):
                continue
            with urllib.request.urlopen(publication_url + "?ws.op=sourceFileUrls", timeout=30) as response:
                urls = json.load(response)
            descriptors = [url for url in urls if urllib.parse.urlsplit(url).scheme == "https" and urllib.parse.urlsplit(url).hostname == "launchpad.net" and urllib.parse.unquote(urllib.parse.urlsplit(url).path).split("/")[-1] == filename]
            if not descriptors:
                continue
            url = descriptors[0]
            if descriptor.exists():
                with urllib.request.urlopen(url, timeout=30) as response:
                    remote = response.read(1048576)
                if descriptor.read_bytes() != remote:
                    raise RuntimeError("Cached source descriptor differs from official archive")
            else:
                download(url, descriptor)
            base = url.rsplit("/", 1)[0] + "/"
            break
        if base is None:
            raise RuntimeError(f"Exact source unavailable: {name} {version}")
    signature = subprocess.run(["gpgv", "--keyring", args.keyring, str(descriptor)], capture_output=True, text=True)
    if signature.returncode and (args.require_signature or "No public key" not in signature.stderr):
        raise RuntimeError(f"Source signature validation failed: {name}")
    text = descriptor.read_text()
    if f"Source: {name}\n" not in text or f"Version: {version}\n" not in text:
        raise RuntimeError("Signed source identity mismatch")
    match = re.search(r"^Checksums-Sha256:\n((?: [^\n]+\n)+)", text, re.M)
    if not match:
        raise RuntimeError("Signed source lacks SHA256 archive inventory")
    files = []
    for line in match[1].splitlines():
        digest, size, archive = line.split()
        if not re.fullmatch(r"[0-9a-f]{64}", digest) or not size.isdigit():
            raise RuntimeError("Invalid signed source archive hash/size")
        if pathlib.PurePosixPath(archive).name != archive:
            raise RuntimeError("Unsafe source archive filename")
        target = directory / archive
        download(base + urllib.parse.quote(archive), target, digest)
        if target.stat().st_size != int(size):
            raise RuntimeError("Source archive size mismatch")
        files.append({"file": archive, "sha256": digest, "size": int(size)})
    records = [record for record in records if (record["source_package"], record["source_version"]) != (name, version)]
    records.append({"source_package": name, "source_version": version, "signature_verified": signature.returncode == 0, "descriptor_transport": "HTTPS from official Ubuntu archive/Launchpad; uploader key may not be installed", "source_base_url": base, "descriptor": filename, "descriptor_sha256": hashlib.sha256(descriptor.read_bytes()).hexdigest(), "archives": files})
    (args.destination / "SOURCE_ARCHIVES.json").write_text(json.dumps(records, indent=2) + "\n")
    print(f"SOURCE_VERIFIED {name} {version} archives={len(files)}", flush=True)
