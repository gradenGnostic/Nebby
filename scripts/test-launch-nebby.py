#!/usr/bin/env python3
"""Portable launcher paths, arguments and missing-tool diagnostics."""
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest


class LauncherTests(unittest.TestCase):
    def test_paths_spaces_and_missing_tool(self):
        with tempfile.TemporaryDirectory(prefix="nebby launcher ") as directory:
            root = pathlib.Path(directory)
            script = root / "LaunchNebby.sh"
            shutil.copy2(pathlib.Path(__file__).with_name("LaunchNebby.sh"), script)
            launcher = root / "nebby-ui"
            launcher.write_text("#!/usr/bin/env python3\nimport json,os,sys\nprint(json.dumps([os.getcwd(),os.environ['NEBBY_APP_ROOT'],os.environ['POKEMOON_WORKSPACE'],sys.argv[1:]]))\n")
            launcher.chmod(0o755)
            for relative in ("tools/3dsrecomp/3dsrecomp", "runtimes/zakuro/zakuro"):
                tool = root / relative
                tool.parent.mkdir(parents=True, exist_ok=True)
                tool.write_text("#!/bin/sh\nexit 0\n")
                tool.chmod(0o755)
            environment = {key:value for key,value in os.environ.items() if key not in {"POKEMOON_WORKSPACE","NEBBY_APP_ROOT","NEBBY_DATA_DIR"}}
            result = subprocess.run(["bash",str(script),"argument with spaces"],cwd="/tmp",env=environment,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),[str(root),str(root),str(root/"workspace"),["argument with spaces"]])
            (root/"tools/3dsrecomp/3dsrecomp").chmod(0o644)
            result = subprocess.run(["bash",str(script)],env=environment,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn("Missing executable: tools/3dsrecomp/3dsrecomp",result.stderr)
            mock = root / "host-tools"
            mock.mkdir()
            getconf = mock / "getconf"
            getconf.write_text("#!/bin/sh\necho 'glibc 2.38'\n")
            getconf.chmod(0o755)
            environment["PATH"] = str(mock) + os.pathsep + environment["PATH"]
            result = subprocess.run(["bash",str(script)],env=environment,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn("requires glibc 2.39",result.stderr)


if __name__ == "__main__":
    unittest.main()
