"""Exercise the real GNU timeout wrapper with controlled service-manager clients."""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path


@unittest.skipUnless(shutil.which("timeout"), "requires GNU timeout on Linux")
class SystemdTimeout(unittest.TestCase):
    def execute(self, mode, stop_mode="normal", load_state="loaded"):
        repo = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory(prefix="exitscope-systemd-timeout-") as tmp:
            directory = Path(tmp)
            reports = directory / "reports"
            reports.mkdir()
            clients = {
                "script": """
import os, shlex, sys
command = shlex.split(sys.argv[sys.argv.index('-c') + 1])
os.execvp(command[0], command)
""",
                "systemd-run": """
import json, os, signal, sys
from pathlib import Path
reports = Path(os.environ['TEST_REPORTS'])
(reports / 'arguments.json').write_text(json.dumps(sys.argv[1:]))
(reports / 'diagnostics.txt').write_text('retained before client completion')
print('validation client started', flush=True)
mode = os.environ['TEST_MODE']
if mode == 'hang':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    while True:
        signal.pause()
sys.exit(42 if mode == 'failure' else 0)
""",
                "systemctl": """
import os, signal, sys
from pathlib import Path
reports = Path(os.environ['TEST_REPORTS'])
assert sys.argv[1] == '--user', sys.argv
assert sys.argv[-1] == 'exitscope-validation-regression-1.service', sys.argv
if sys.argv[2] == 'show':
    print(os.environ['TEST_LOAD_STATE'])
elif sys.argv[2] == 'stop':
    (reports / 'stop-called.txt').write_text(sys.argv[-1])
    if os.environ['TEST_STOP_MODE'] == 'hang':
        while True:
            signal.pause()
else:
    raise AssertionError(sys.argv)
""",
            }
            for name, code in clients.items():
                path = directory / name
                path.write_text(f"#!{sys.executable}\n{code}")
                path.chmod(0o755)
            env = dict(
                os.environ,
                PATH=str(directory) + os.pathsep + os.environ["PATH"],
                GITHUB_RUN_ID="regression",
                GITHUB_RUN_ATTEMPT="1",
                EXITSCOPE_SYSTEMD_TIMEOUT="0.3s",
                EXITSCOPE_SYSTEMD_STOP_TIMEOUT="0.3s",
                TEST_REPORTS=str(reports),
                TEST_MODE=mode,
                TEST_STOP_MODE=stop_mode,
                TEST_LOAD_STATE=load_state,
            )
            result = subprocess.run(
                [
                    "sh",
                    str(repo / "scripts/systemd-run-checks.sh"),
                    "/unused/exitscope",
                    str(reports),
                ],
                env=env,
                capture_output=True,
                text=True,
                timeout=9,
            )
            files = {p.name: p.read_text() for p in reports.iterdir()}
            self.assertEqual(
                files["diagnostics.txt"], "retained before client completion"
            )
            arguments = json.loads(files["arguments.json"])
            for flag in [
                "--user",
                "--collect",
                "--pty",
                "--property=Delegate=yes",
                "--property=RuntimeMaxSec=120s",
                "--property=TimeoutStopSec=5s",
            ]:
                self.assertIn(flag, arguments)
            self.assertEqual(arguments[-3], str(repo / "scripts/systemd-checks.sh"))
            return result, files

    def test_successful_collected_unit_is_not_stopped_again(self):
        result, files = self.execute("success", load_state="not-found")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("stop-called.txt", files)
        self.assertEqual(files["unit-cleanup-status.txt"], "0\n")

    def test_failure_preserves_status_and_diagnostics_and_stops_unit(self):
        result, files = self.execute("failure")
        self.assertEqual(result.returncode, 42, result.stderr)
        self.assertIn("stop-called.txt", files)
        self.assertEqual(files["execution-status.txt"], "42\n")

    def test_hung_client_is_killed_and_unit_stopped(self):
        result, files = self.execute("hang")
        # GNU timeout returns 137 when its kill-after limit is needed.
        self.assertEqual(result.returncode, 137, result.stderr)
        self.assertIn("stop-called.txt", files)
        self.assertEqual(files["execution-status.txt"], "137\n")
        self.assertEqual(files["unit-cleanup-status.txt"], "0\n")

    def test_hung_stop_is_bounded_and_cannot_turn_into_success(self):
        result, files = self.execute("success", stop_mode="hang")
        self.assertEqual(result.returncode, 124, result.stderr)
        self.assertEqual(files["execution-status.txt"], "0\n")
        self.assertEqual(files["unit-cleanup-status.txt"], "124\n")


def manager_probe(reports):
    """Validate actual service timeout and cgroup removal on the hosted VM."""
    reports.mkdir(parents=True, exist_ok=True)
    unit = f"exitscope-timeout-probe-{os.getpid()}.service"
    result = subprocess.run([
        "timeout", "--kill-after=2s", "10s", "systemd-run", "--user", "--wait",
        "--collect", f"--unit={unit}", "--property=RuntimeMaxSec=1s",
        "--property=TimeoutStopSec=1s", "sh", "-c",
        'cat /proc/self/cgroup > "$1/cgroup.txt"; exec sleep infinity',
        "probe", str(reports),
    ], capture_output=True, text=True, timeout=15)
    (reports / "stdout.txt").write_text(result.stdout)
    (reports / "stderr.txt").write_text(result.stderr)
    (reports / "status.txt").write_text(str(result.returncode) + "\n")
    # A client timeout is not proof that the service manager stopped the unit.
    assert result.returncode not in (0, 124, 137), result
    cgroup = next(line[3:] for line in (reports / "cgroup.txt").read_text().splitlines()
                  if line.startswith("0::"))
    deadline = time.monotonic() + 3
    while True:
        state = subprocess.run([
            "timeout", "--kill-after=2s", "5s", "systemctl", "--user", "show",
            "--property=LoadState", "--value", unit,
        ], capture_output=True, text=True, timeout=10)
        removed = not (Path("/sys/fs/cgroup") / cgroup.lstrip("/")).exists()
        if (state.returncode == 0 and state.stdout.strip() == "not-found" and removed
                or time.monotonic() >= deadline):
            break
        time.sleep(0.05)
    (reports / "unit-load-state.txt").write_text(state.stdout)
    assert state.returncode == 0 and state.stdout.strip() == "not-found", state
    assert removed, cgroup
    print("real systemd RuntimeMaxSec probe: failed service collected, cgroup removed")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--manager-probe":
        manager_probe(Path(sys.argv[2]).resolve())
    else:
        unittest.main(verbosity=2)
