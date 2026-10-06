#!/usr/bin/env python3
"""Synthetic notice fixture; no game data or donor mutations."""
import contextlib
import io
import json
import pathlib
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import patch


class NoticeTests(unittest.TestCase):
    def test_donor_notice_copies_referenced_license(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            stage = root / "stage"
            (stage / "lib").mkdir(parents=True)
            (stage / "lib/libfmt.so.9").write_bytes(b"synthetic fixture")
            donor = root / "donor"
            notices = donor / "static-recomp-work/native-renderer/deps/usr/share/doc/libfmt9"
            notices.mkdir(parents=True)
            (notices / "copyright").write_text("Fixture: /usr/share/common-licenses/GPL-2.\n")
            script = pathlib.Path(__file__).with_name("collect-dependency-notices.py")
            empty = subprocess.CompletedProcess([], 1, "", "")
            with patch("sys.argv", [str(script), str(stage), str(donor)]), patch("subprocess.run", return_value=empty), contextlib.redirect_stdout(io.StringIO()):
                runpy.run_path(str(script), run_name="__main__")
            output = stage / "licenses/desktop-dependencies"
            self.assertEqual((output / "libfmt9/GPL-2").read_bytes(), pathlib.Path("/usr/share/common-licenses/GPL-2").read_bytes())
            record = json.loads((output / "inventory.json").read_text())[0]
            self.assertEqual(record["notice_packages"], ["libfmt9"])
            self.assertEqual(record["installed_source_metadata"], [])
            self.assertEqual(len(record["sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
