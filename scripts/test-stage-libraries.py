#!/usr/bin/env python3
"""Protect original binaries when audit candidates use hardlinks."""
import os
import pathlib
import subprocess
import tempfile
import unittest


class LibraryStageTests(unittest.TestCase):
    def test_shared_binary_refused_before_tools_execute(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            original = root / "original"
            original.write_bytes(b"valuable original fixture")
            stage = root / "stage"
            for relative in ("nebby-ui", "tools/3dsrecomp/3dsrecomp", "runtimes/zakuro/zakuro"):
                target = stage / relative
                target.parent.mkdir(parents=True, exist_ok=True)
                os.link(original, target)
            script = pathlib.Path(__file__).with_name("stage-libraries.py")
            result = subprocess.run(["python3", str(script), str(stage), "nonexistent-patcher"], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Refusing to patch shared hardlink", result.stderr)
            self.assertEqual(original.read_bytes(), b"valuable original fixture")


if __name__ == "__main__":
    unittest.main()
