"""Exercise documented diagnostics without a real service manager or secrets."""

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path


class Documentation(unittest.TestCase):
    def test_systemd_diagnostics_do_not_save_manager_environment(self):
        repo = Path(__file__).resolve().parents[1]
        report = (repo / "docs/engineering-report.md").read_text()
        match = re.search(
            r'^\{ cat /etc/os-release;.*?^\} > "\$reports/environment.txt"',
            report,
            flags=re.MULTILINE | re.DOTALL,
        )
        self.assertIsNotNone(match, "documented environment diagnostics are missing")
        with tempfile.TemporaryDirectory(prefix="exitscope-docs-") as tmp:
            directory = Path(tmp)
            (directory / "cgroup.procs").touch()
            manager = directory / "systemctl"
            manager.write_text(
                '#!/bin/sh\n'
                'printf "%s\\n" "$*" > "$EXITSCOPE_PROBE_CALL"\n'
                'printf "EXITSCOPE_TEST_SECRET=%s\\n" "$EXITSCOPE_TEST_SECRET"\n'
            )
            manager.chmod(0o755)
            sentinel = "dummy-manager-secret-for-regression"
            env = dict(
                os.environ,
                PATH=str(directory) + os.pathsep + os.defpath,
                reports=tmp,
                parent=tmp,
                EXITSCOPE_PROBE_CALL=str(directory / "called"),
                EXITSCOPE_TEST_SECRET=sentinel,
            )
            result = subprocess.run(
                ["sh", "-c", match.group()], env=env,
                capture_output=True, text=True, timeout=5, check=False,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((directory / "called").read_text(), "--user show-environment\n")
            diagnostics = (directory / "environment.txt").read_text()
            self.assertTrue(diagnostics, "diagnostics must still be collected")
            for output in [diagnostics, result.stdout, result.stderr]:
                self.assertNotIn(sentinel, output)


if __name__ == "__main__":
    unittest.main(verbosity=2)
