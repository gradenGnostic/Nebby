#!/usr/bin/env python3
"""Desktop staging uses checkout-local tools and includes every title."""
import pathlib
import shutil
import subprocess
import tempfile
import unittest


class DesktopStageTests(unittest.TestCase):
    def test_checkout_tools_and_all_titles(self):
        with tempfile.TemporaryDirectory(prefix="nebby-stage-test-") as directory:
            root = pathlib.Path(directory) / "checkout"
            files = {
                "target/release/nebby-ui": "launcher",
                "tools/3dsrecomp/3dsrecomp": "recompiler",
                "runtimes/zakuro/zakuro": "runtime",
                "titles/moon.json": "{}",
                "titles/alpha-sapphire.json": "{}",
                "mods/catalog.json": "{}",
                "licenses/NOTICE": "notice",
                "LICENSE": "license",
                "THIRD_PARTY_NOTICES.md": "notices",
                "scripts/LaunchNebby.sh": "#!/bin/sh\n",
                "assets/steamgriddb/test-heroes.png": "artwork",
                "assets/steamgriddb/PROVENANCE.json": "[]",
                "assets/nebby.png": "icon",
            }
            for relative, content in files.items():
                path = root / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            script = root / "scripts/stage-desktop.sh"
            shutil.copy2(pathlib.Path(__file__).with_name("stage-desktop.sh"), script)
            stage = pathlib.Path(directory) / "release"
            result = subprocess.run(["bash", str(script), str(stage)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((stage / "tools/3dsrecomp/3dsrecomp").read_text(), "recompiler")
            self.assertEqual((stage / "runtimes/zakuro/zakuro").read_text(), "runtime")
            self.assertTrue((stage / "titles/alpha-sapphire.json").is_file())
            self.assertEqual((stage / "assets/steamgriddb/test-heroes.png").read_text(), "artwork")
            self.assertTrue((stage / "assets/steamgriddb/PROVENANCE.json").is_file())
            self.assertEqual((stage / "assets/nebby.png").read_text(), "icon")
            self.assertFalse((stage / "android").exists())
            again = subprocess.run(["bash", str(script), str(stage)], capture_output=True)
            self.assertNotEqual(again.returncode, 0)


if __name__ == "__main__":
    unittest.main()
