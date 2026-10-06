#!/usr/bin/env python3
"""Source export regression fixtures; no retail data."""
import json
import pathlib
import subprocess
import tempfile
import unittest

SCRIPT = pathlib.Path(__file__).with_name("export-sdk-dependencies.py")


class ExportTests(unittest.TestCase):
    def test_sources_patches_notices_and_safe_resume(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            donor = root / "donor"
            source = donor / "static-recomp-work/native-renderer/build-probe/_deps/nri-src"
            source.mkdir(parents=True)
            (source / "CMakeLists.txt").write_text("project(Synthetic)\n")
            (source / "LICENSE.txt").write_text("Synthetic notice fixture\n")
            (source / "renderer.cpp").write_text("int fixture = 1;\n")
            subprocess.run(["git", "init", "-q", str(source)], check=True)
            subprocess.run(["git", "-C", str(source), "add", "."], check=True)
            subprocess.run(["git", "-C", str(source), "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", "commit", "-qm", "fixture"], check=True)
            (source / "renderer.cpp").write_text("int fixture = 2;\n")
            (source / "ignored.o").write_bytes(b"not a source file")
            stage = root / "stage"
            stage.mkdir()
            command = ["python3", str(SCRIPT), str(donor), str(stage), "--names", "nri"]
            subprocess.run(command, check=True, capture_output=True)
            output = stage / "workspace/static-recomp-work/native-renderer/vendor"
            self.assertEqual((output / "nri/renderer.cpp").read_text(), "int fixture = 2;\n")
            self.assertTrue((output / "nri/LICENSE.txt").is_file())
            self.assertFalse((output / "nri/.git").exists())
            self.assertFalse((output / "nri/ignored.o").exists())
            provenance = json.loads((output / "PROVENANCE.json").read_text())[0]
            self.assertTrue(provenance["commit"])
            self.assertIn(" M renderer.cpp", provenance["local_changes"])
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 1)
            subprocess.run(command + ["--resume"], check=True, capture_output=True)
            (output / "nri/renderer.cpp").write_text("valuable existing change\n")
            rejected = subprocess.run(command + ["--resume"], capture_output=True)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertEqual((output / "nri/renderer.cpp").read_text(), "valuable existing change\n")


if __name__ == "__main__":
    unittest.main()
