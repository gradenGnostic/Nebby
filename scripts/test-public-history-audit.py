#!/usr/bin/env python3
"""Current-tree success must not hide assets retained in Git ancestry."""
import pathlib
import subprocess
import tempfile
import unittest
import shutil


class HistoryTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("cc"), "C compiler required")
    def test_generic_library_rejects_retail_entry_table(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            def git(*args):
                return subprocess.run(["git", "-C", str(root), *args], check=True, capture_output=True)
            git("init", "-q")
            for name in ("README.md", "LICENSE", "THIRD_PARTY_NOTICES.md", "assets/ATTRIBUTION.md"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture")
            library = root / "lib/libpulsecommon-16.1.so"
            library.parent.mkdir()
            subprocess.run(["cc", "-shared", "-fPIC", "-x", "c", "-", "-o", str(library)], input="int native_helper(void){return 1;}\n", text=True, check=True)
            git("add", ".")
            audit = pathlib.Path(__file__).with_name("audit-public.py")
            command = ["python3", str(audit), "--repo", str(root), "--current-tree-only"]
            self.assertEqual(subprocess.run(command, capture_output=True).returncode, 0)
            subprocess.run(["cc", "-shared", "-fPIC", "-x", "c", "-", "-o", str(library)], input="int recomp_entries=1;\n", text=True, check=True)
            rejected = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(rejected.returncode, 0)
            self.assertIn("Translated retail code", rejected.stdout)

    def test_removed_asset_remains_a_public_history_blocker(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            def git(*args):
                return subprocess.run(["git", "-C", str(root), "-c", "user.name=Fixture", "-c", "user.email=fixture@localhost", *args], check=True, capture_output=True)
            git("init", "-q")
            for name in ("README.md", "LICENSE", "THIRD_PARTY_NOTICES.md", "assets/ATTRIBUTION.md", "assets/nebby.png"):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"synthetic fixture; not actual artwork")
            git("add", ".")
            git("commit", "-qm", "fixture original")
            git("rm", "--cached", "assets/nebby.png")
            git("commit", "-qm", "fixture excluded current asset")
            audit = pathlib.Path(__file__).with_name("audit-public.py")
            command = ["python3", str(audit), "--repo", str(root)]
            current = subprocess.run(command + ["--current-tree-only"], capture_output=True, text=True)
            self.assertEqual(current.returncode, 0, current.stdout)
            history = subprocess.run(command, capture_output=True, text=True)
            self.assertNotEqual(history.returncode, 0)
            self.assertIn("Unlicensed legacy branding in reachable Git history", history.stdout)
            self.assertTrue((root / "assets/nebby.png").is_file())


if __name__ == "__main__":
    unittest.main()
