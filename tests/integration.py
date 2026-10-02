"""Linux real-process acceptance suite. Requires a disposable delegated subtree.

Stores every run, including failed assertions, before evaluating expectations.
"""

import argparse
import errno
import json
import os
import signal
import subprocess
import tempfile
import time
import unittest
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--binary", type=Path, default=Path("target/debug/exitscope"))
parser.add_argument("--cgroup-parent", type=Path, required=True)
parser.add_argument("--reports", type=Path, default=Path("validation/synthetic"))
args = parser.parse_args()
args.reports = args.reports / str(time.time_ns())
binary = args.binary.resolve()
repo = Path(__file__).resolve().parents[1]
args.reports.mkdir(parents=True, exist_ok=True)


class Integration(unittest.TestCase):
    def profile(
        self,
        mode="correct",
        behavior="normal",
        scenario="parent_sigterm",
        readiness="fixture",
    ):
        graceful = scenario == "parent_sigterm"
        return {
            "executable": "python3",
            "args": [str(repo / "tests/wrapper.py"), mode, behavior],
            "cwd": str(repo),
            "scenario": scenario,
            "readiness": readiness,
            "readiness_ms": 1200,
            "shutdown_ms": 500,
            "output_ms": 600,
            "contract": {
                "signal_receipt": graceful,
                "cleanup_finished": graceful,
                "root_waits_cleanup": graceful,
                "no_survivors": True,
                "output_eof": True,
            },
        }

    def execute(self, profile, parent=None, interrupt=False, label=None):
        with tempfile.TemporaryDirectory(prefix="exitscope-tests-") as tmp:
            config = Path(tmp) / "profile.json"
            config.write_text(json.dumps(profile))
            marker = Path(tmp) / "marker"
            env = dict(os.environ, EXITSCOPE_TEST_MARKER=str(marker))
            command = [
                str(binary),
                "run",
                "--config",
                str(config),
                "--cgroup-parent",
                str(parent or args.cgroup_parent),
                "--json",
            ]
            process = subprocess.Popen(
                command,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env,
            )
            if interrupt:
                deadline = time.monotonic() + 3
                while (
                    not marker.exists()
                    and process.poll() is None
                    and time.monotonic() < deadline
                ):
                    time.sleep(0.002)
                process.send_signal(signal.SIGINT)
            stdout, stderr = process.communicate(timeout=10)
            report = json.loads(stdout)
            report_path = args.reports / f"{label or self.id().split('.')[-1]}.json"
            report_path.write_text(stdout)
            (report_path.with_suffix(".stderr.txt")).write_text(stderr)
            self.assertEqual(
                process.returncode,
                {"PASS": 0, "FAIL": 1, "UNRESOLVED": 2, "INFRASTRUCTURE_ERROR": 3}[
                    report["outcome"]
                ],
            )
            if report["cgroup"]:
                self.assertLessEqual(
                    report["verdict_at_ms"], report["cleanup_started_at_ms"]
                )
                self.assertLessEqual(
                    report["cleanup_started_at_ms"], report["cleanup_finished_at_ms"]
                )
                self.assertFalse(
                    Path(report["cgroup"]).exists(),
                    "cleanup must remove this run's cgroup",
                )
            return report

    def ids(self, report):
        return {f["id"] for f in report["findings"]}

    def test_correct_forwarding(self):
        r = self.execute(self.profile())
        self.assertEqual(r["outcome"], "PASS")
        self.assertEqual(
            [e["kind"] for e in r["events"] if e["kind"] != "gated"],
            [
                "handler_installed",
                "ready",
                "signal_received",
                "cleanup_started",
                "cleanup_finished",
            ],
        )

    def test_missing_forwarding_and_pipes(self):
        r = self.execute(self.profile("missing_forward"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertTrue(
            {"SIGNAL_NOT_RECEIVED", "LIVE_MEMBERS", "STDOUT_OPEN", "STDERR_OPEN"}
            <= self.ids(r)
        )
        self.assertIsNotNone(r["root_exit"])

    def test_premature_worker(self):
        r = self.execute(self.profile(behavior="premature"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIn("CLEANUP_INCOMPLETE", self.ids(r))

    def test_wrong_signal(self):
        r = self.execute(self.profile("wrong_signal"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIn("WRONG_SIGNAL", self.ids(r))
        self.assertTrue(any(e["signal"] == signal.SIGINT for e in r["events"]))

    def test_root_does_not_wait(self):
        r = self.execute(self.profile("early_root"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIn("ROOT_DID_NOT_WAIT", self.ids(r))

    def test_group_kill(self):
        r = self.execute(self.profile(scenario="group_sigkill"))
        self.assertEqual(r["outcome"], "PASS")
        self.assertEqual(r["root_exit"]["signal"], signal.SIGKILL)

    def test_survivor_other_session(self):
        r = self.execute(self.profile(behavior="new_session", scenario="group_sigkill"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertTrue(any(p["sid"] != r["root"]["sid"] for p in r["survivors"]))
        self.assertIn("STDOUT_OPEN", self.ids(r))

    def test_survivor_other_group(self):
        r = self.execute(self.profile(behavior="new_group", scenario="group_sigkill"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertTrue(any(p["pgid"] != r["root"]["pgid"] for p in r["survivors"]))

    def test_survivors_independent_of_eof(self):
        r = self.execute(self.profile("missing_forward", "close_output"))
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIn("LIVE_MEMBERS", self.ids(r))
        self.assertTrue(r["stdout"]["eof_before_deadline"])

    def test_readiness_failure(self):
        r = self.execute(self.profile(behavior="no_ready"))
        self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")
        self.assertIn("READINESS_TIMEOUT", self.ids(r))
        self.assertIsNone(r["impact_at_ms"])

    def test_missing_telemetry(self):
        p = self.profile(readiness="exec")
        p.update(executable="python3", args=["-c", "import signal; signal.pause()"])
        r = self.execute(p)
        self.assertEqual(r["outcome"], "UNRESOLVED")
        self.assertIn("TELEMETRY_MISSING", self.ids(r))

    def test_exec_failure(self):
        p = self.profile()
        p["executable"] = "/no/such/exitscope-command"
        r = self.execute(p)
        self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")
        self.assertEqual(r["launch_error"], {"stage": "exec", "errno": errno.ENOENT})

    def test_bad_working_directory(self):
        p = self.profile()
        p["cwd"] = "/no/such/exitscope-directory"
        r = self.execute(p)
        self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")
        self.assertEqual(
            r["launch_error"], {"stage": "working_directory", "errno": errno.ENOENT}
        )

    def test_exec_permission_denied(self):
        with tempfile.TemporaryDirectory() as tmp:
            executable = Path(tmp) / "not-executable"
            executable.write_text("fixture")
            executable.chmod(0o644)
            p = self.profile()
            p["executable"] = str(executable)
            r = self.execute(p)
            self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")
            self.assertEqual(
                r["launch_error"], {"stage": "exec", "errno": errno.EACCES}
            )

    def test_interruption(self):
        r = self.execute(self.profile(behavior="no_ready"), interrupt=True)
        self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")
        self.assertIn("RUNNER_INTERRUPTED", self.ids(r))

    def test_foreign_process_untouched(self):
        # Pidfd signal and cgroup.kill must never target this process outside the run.
        foreign = subprocess.Popen(["sleep", "30"])
        try:
            r = self.execute(
                self.profile(behavior="new_session", scenario="group_sigkill")
            )
            self.assertEqual(r["outcome"], "FAIL")
            self.assertIsNone(foreign.poll())
            self.assertNotIn(foreign.pid, [p["pid"] for p in r["survivors"]])
        finally:
            foreign.terminate()
            foreign.wait(timeout=3)

    def test_not_cgroup(self):
        with tempfile.TemporaryDirectory() as tmp:
            r = self.execute(self.profile(), parent=tmp)
            self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")

    def test_no_permissions(self):
        # Even root cannot create a child in an explicitly read-only cgroup bind mount.
        readonly = os.environ.get("EXITSCOPE_READONLY_CGROUP")
        if not readonly:
            self.skipTest("set EXITSCOPE_READONLY_CGROUP to a read-only cgroup mount")
        r = self.execute(self.profile(), parent=readonly)
        self.assertEqual(r["outcome"], "INFRASTRUCTURE_ERROR")

    def test_output_flood_does_not_starve(self):
        r = self.execute(self.profile(behavior="flood"))
        self.assertEqual(r["outcome"], "PASS")
        self.assertGreater(r["stdout"]["bytes"], 0)

    def test_late_cleanup_cannot_pass(self):
        p = self.profile()
        p["shutdown_ms"] = 40
        r = self.execute(p)
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIn("CLEANUP_DEADLINE", self.ids(r))

    def test_optional_late_cleanup(self):
        p = self.profile("early_root")
        p["shutdown_ms"] = 80
        p["contract"].update(
            cleanup_finished=False, root_waits_cleanup=False, no_survivors=False
        )
        r = self.execute(p)
        self.assertEqual(r["outcome"], "PASS")
        self.assertNotIn("CLEANUP_DEADLINE", self.ids(r))
        self.assertTrue(r["stdout"]["eof_before_deadline"])
        self.assertTrue(
            any(
                e["kind"] == "cleanup_finished"
                and e["at_ms"] > r["impact_at_ms"] + p["shutdown_ms"]
                for e in r["events"]
            )
        )

    def test_live_root_during_unfinished_cleanup(self):
        p = self.profile()
        p.update(shutdown_ms=40, output_ms=40)
        r = self.execute(p)
        self.assertEqual(r["outcome"], "FAIL")
        self.assertIsNone(r["root_exit"])
        self.assertNotIn("ROOT_DID_NOT_WAIT", self.ids(r))
        self.assertIn("CLEANUP_INCOMPLETE", self.ids(r))

    def test_intentional_background_contract(self):
        p = self.profile("missing_forward")
        p["contract"] = {
            "signal_receipt": False,
            "cleanup_finished": False,
            "root_waits_cleanup": False,
            "no_survivors": False,
            "output_eof": False,
        }
        r = self.execute(p)
        self.assertEqual(r["outcome"], "PASS")
        self.assertTrue(r["survivors"])


if __name__ == "__main__":
    unittest.main(argv=[__file__], verbosity=2)
