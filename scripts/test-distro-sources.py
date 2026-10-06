#!/usr/bin/env python3
"""Exact source collection tests using synthetic HTTPS/PGP fixtures."""
import contextlib
import hashlib
import io
import json
import pathlib
import runpy
import subprocess
import tempfile
import unittest
import urllib.error
from unittest.mock import patch

SCRIPT = pathlib.Path(__file__).with_name("collect-distro-sources.py")


class SourceTests(unittest.TestCase):
    def test_exact_hash_collection_and_tampered_cache_rejection(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            inventory = root / "inventory.json"
            inventory.write_text(json.dumps([{"installed_source_metadata": [{"source_package": "fixture", "source_version": "1.0", "staged_build_id_matches": True}]}]))
            output = root / "sources"
            archive = b"synthetic source archive, not retail content"
            digest = hashlib.sha256(archive).hexdigest()
            descriptor = f"Source: fixture\nVersion: 1.0\nChecksums-Sha256:\n {digest} {len(archive)} fixture_1.0.orig.tar.xz\n".encode()
            def open_url(url, timeout):
                return io.BytesIO(descriptor if url.endswith(".dsc") else archive)
            missing_key = subprocess.CompletedProcess([], 2, "", "No public key")
            argv = [str(SCRIPT), str(inventory), str(output), "--package", "fixture"]
            def run():
                with patch("sys.argv", argv), patch("urllib.request.urlopen", side_effect=open_url), patch("subprocess.run", return_value=missing_key), contextlib.redirect_stdout(io.StringIO()):
                    runpy.run_path(str(SCRIPT), run_name="__main__")
            run()
            record = json.loads((output / "SOURCE_ARCHIVES.json").read_text())[0]
            self.assertFalse(record["signature_verified"])
            self.assertEqual(record["archives"][0]["sha256"], digest)
            run()  # Identical cache is reusable; still checked against HTTPS descriptor.
            cached = output / "fixture-1.0/fixture_1.0.dsc"
            cached.write_bytes(b"valuable changed fixture")
            with self.assertRaisesRegex(RuntimeError, "Cached source descriptor differs"):
                run()
            self.assertEqual(cached.read_bytes(), b"valuable changed fixture")
            cached.write_bytes(descriptor)
            archive_path = output / "fixture-1.0/fixture_1.0.orig.tar.xz"
            archive_path.write_bytes(b"changed archive")
            with self.assertRaisesRegex(RuntimeError, "Refusing unchecked overwrite"):
                run()
            self.assertEqual(archive_path.read_bytes(), b"changed archive")
            archive_path.write_bytes(archive)
            normal_open = open_url
            def open_url(url, timeout):
                if "archive.ubuntu.com/ubuntu/pool/" in url:
                    raise urllib.error.HTTPError(url, 404, "superseded", None, None)
                if "getPublishedSources" in url:
                    return io.BytesIO(json.dumps({"entries": [{"self_link": "https://api.launchpad.net/1.0/ubuntu/+archive/primary/+sourcepub/1"}]}).encode())
                if "sourceFileUrls" in url:
                    return io.BytesIO(json.dumps(["https://launchpad.net/ubuntu/+archive/primary/+sourcefiles/fixture/1.0/fixture_1.0.dsc"]).encode())
                return normal_open(url, timeout)
            run()
            record = json.loads((output / "SOURCE_ARCHIVES.json").read_text())[0]
            self.assertTrue(record["source_base_url"].startswith("https://launchpad.net/"))


if __name__ == "__main__":
    unittest.main()
