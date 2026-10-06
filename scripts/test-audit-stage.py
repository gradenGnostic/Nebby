#!/usr/bin/env python3
"""Public package guard regression tests; uses synthetic data only."""
import pathlib
import subprocess
import tempfile
import unittest
import shutil
import json

AUDIT = pathlib.Path(__file__).with_name("audit-stage.py")
DEVELOPER_ROOT = pathlib.PurePosixPath("/", "home", "developer")

class PackageGateTests(unittest.TestCase):
    def test_missing_or_empty_package_fails(self):
        with tempfile.TemporaryDirectory(prefix="nebby-missing-package-") as directory:
            for root in [pathlib.Path(directory), pathlib.Path(directory) / "missing"]:
                result = subprocess.run(["python3", str(AUDIT), str(root)], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Package directory missing or empty", result.stdout)

    def check_files(self, files):
        with tempfile.TemporaryDirectory(prefix="nebby-package-gate-") as directory:
            root = pathlib.Path(directory)
            for relative in files:
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(files[relative] if isinstance(files, dict) else b"synthetic fixture, not game content")
            return subprocess.run(["python3", str(AUDIT), str(root)], capture_output=True, text=True)

    def test_clean_source_and_notices_pass(self):
        result = self.check_files(["README.md", "licenses/LICENSE", "workspace/runtime/bridge.cpp"])
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_private_artifacts_fail(self):
        for artifact in ["data/profiles/alola/saves/main", "data/config/library.json", "cache/recomp/librecomp.a", "game.cxi", "game.3ds", "keys.keys", "workspace/target/runtime", "workspace/moon-target/runtime", "logs/run.log"]:
            with self.subTest(artifact=artifact):
                result = self.check_files([artifact])
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertIn("FAIL", result.stdout)

    def test_declared_cargo_build_source_required(self):
        manifest = b'[package]\nname="fixture"\nversion="0.1.0"\nbuild="build.rs"\n'
        rejected = self.check_files({"workspace/Cargo.toml": manifest})
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("Missing declared Cargo build source", rejected.stdout)
        accepted = self.check_files({"workspace/Cargo.toml": manifest, "workspace/build.rs": b"fn main() {}"})
        self.assertEqual(accepted.returncode, 0, accepted.stdout)

    def test_developer_configuration_path_rejected(self):
        rejected = self.check_files({"titles/profile.json": json.dumps({"tool": str(DEVELOPER_ROOT / "tool")}).encode()})
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("Developer configuration path", rejected.stdout)

    def test_khronos_profile_path_cannot_hide_private_data(self):
        rejected = self.check_files({"workspace/static-recomp-work/native-renderer/vendor/vulkan_headers/registry/profiles/VP_KHR_roadmap.json": b'{"save":"private"}'})
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("Private data or build cache", rejected.stdout)

    @unittest.skipUnless(shutil.which("cc"), "ELF fixture requires host C compiler")
    def test_real_elf_search_paths(self):
        for rpath, expected in [(str(DEVELOPER_ROOT / "lib"), 1), ("/tmp/staged/lib", 1), ("/dev/shm/staged/lib", 1), ("$ORIGIN/lib:/tmp/staged/lib", 1), ("$ORIGIN/lib", 0)]:
            with self.subTest(rpath=rpath), tempfile.TemporaryDirectory(prefix="nebby-elf-gate-") as directory:
                root = pathlib.Path(directory)
                source = root / "fixture.c"
                source.write_text("int main(void) { return 0; }\n")
                subprocess.run(["cc", str(source), f"-Wl,-rpath,{rpath}", "-o", str(root / "fixture")], check=True, capture_output=True)
                result = subprocess.run(["python3", str(AUDIT), str(root)], capture_output=True, text=True)
                self.assertEqual(result.returncode, expected, result.stdout)

if __name__ == "__main__":
    unittest.main()
