#!/usr/bin/env python3
"""Package resource changes and unsafe manifest paths are rejected."""
import hashlib
import importlib.util
import json
import pathlib
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("verify_manifest", pathlib.Path(__file__).with_name("verify-package-manifest.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ManifestTests(unittest.TestCase):
    def test_resource_changes_and_unsafe_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            root = pathlib.Path(directory)
            resource = root / "title.json"
            resource.write_bytes(b"fixture")
            digest = hashlib.sha256(b"fixture").hexdigest()
            tree = hashlib.sha256(b"title.json\0" + digest.encode() + b"\n").hexdigest()
            manifest = {"schema":1,"managed_artifacts":[],"package_files":1,"package_tree_sha256":tree}
            path = root / "PACKAGE_MANIFEST.json"
            path.write_text(json.dumps(manifest))
            self.assertEqual(module.verify(root),1)
            resource.write_bytes(b"changed")
            with self.assertRaisesRegex(ValueError,"content changed"):
                module.verify(root)
            resource.write_bytes(b"fixture")
            manifest["managed_artifacts"] = [{"path":"../outside","size":0,"sha256":""}]
            path.write_text(json.dumps(manifest))
            with self.assertRaisesRegex(ValueError,"Unsafe artifact path"):
                module.verify(root)


if __name__ == "__main__":
    unittest.main()
