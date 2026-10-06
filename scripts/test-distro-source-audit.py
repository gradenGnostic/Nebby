#!/usr/bin/env python3
"""Source coverage regression tests with synthetic, non-retail artifacts."""
import hashlib
import json
import pathlib
import subprocess
import tempfile
import unittest


class AuditTests(unittest.TestCase):
    def test_complete_then_changed_source_set(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            sources = root / "licenses/corresponding-sources"
            folder = sources / "fixture-1.0"
            folder.mkdir(parents=True)
            binary_notices = root / "licenses/desktop-dependencies"
            binary_notices.mkdir()
            (binary_notices / "inventory.json").write_text(json.dumps([{"library": "fixture.so", "installed_source_metadata": [{"source_package": "fixture", "source_version": "1.0", "staged_build_id_matches": True}]}]))
            descriptor = b"synthetic descriptor"
            archive = b"synthetic source"
            (folder / "fixture.dsc").write_bytes(descriptor)
            source_file = folder / "fixture.tar.xz"
            source_file.write_bytes(archive)
            record = {"source_package": "fixture", "source_version": "1.0", "descriptor": "fixture.dsc", "descriptor_sha256": hashlib.sha256(descriptor).hexdigest(), "archives": [{"file": "fixture.tar.xz", "sha256": hashlib.sha256(archive).hexdigest(), "size": len(archive)}]}
            (sources / "SOURCE_ARCHIVES.json").write_text(json.dumps([record]))
            script = pathlib.Path(__file__).with_name("audit-distro-sources.py")
            command = ["python3", str(script), str(root)]
            valid = subprocess.run(command, capture_output=True, text=True)
            self.assertEqual(valid.returncode, 0, valid.stdout)
            self.assertIn("source_sets=1/1", valid.stdout)
            source_file.write_bytes(b"modified source")
            invalid = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(invalid.returncode, 0)
            self.assertIn("Incomplete/changed source", invalid.stdout)


if __name__ == "__main__":
    unittest.main()
