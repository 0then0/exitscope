"""Real Linux races: queued target telemetry and a delayed deadline observer.

Readiness is acknowledged IPC. Marker files synchronize deliberate observer pauses;
waits after readiness exercise deadline behavior, never substitute for readiness.
"""

import argparse
import json
import os
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def event(connection, kind, signum=None, ack=True):
    wire = {"kind": kind}
    if signum is not None:
        wire["signal"] = signum
    connection.sendall((json.dumps(wire) + "\n").encode())
    if ack:
        data = b""
        while not data.endswith(b"\n"):
            chunk = connection.recv(4 - len(data))
            if not chunk:
                raise RuntimeError("ACK channel closed")
            data += chunk
        assert data == b"ack\n"


def wait_until(predicate, process=None):
    end = time.monotonic() + 5
    while not predicate():
        if process is not None:
            assert process.poll() is None, "runner exited before synchronization"
        assert time.monotonic() < end, "synchronization deadline exceeded"
        time.sleep(0.002)


def worker(mode, directory):
    signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    connection.settimeout(5)
    connection.connect(os.environ["EXIT_SCOPE_SOCKET"])
    event(connection, "handler_installed")
    event(connection, "ready")
    (directory / "ready").write_text(str(os.getpid()))
    if mode.startswith("receipt-"):
        signum = signal.sigwait({signal.SIGTERM})
        if mode != "receipt-timely":
            # Deliberately delay reporting AFTER actual receipt. This proves no
            # kernel delivery claim; 1.3 s gives 800 ms beyond shutdown_ms.
            time.sleep(1.3)
        event(connection, "signal_received", signum)
        return
    if mode == "late":
        # Escaped session lives well beyond the 200 ms shutdown deadline.
        time.sleep(0.8)
        (directory / "expired").touch()
        return
    signum = signal.sigwait({signal.SIGTERM})
    event(connection, "signal_received", signum)
    (directory / "signal_acked").touch()
    wait_until(lambda: (directory / "continue").exists())
    if mode == "truncated":
        connection.sendall(b'{"kind":"cleanup_sta')
    else:
        event(connection, "cleanup_started", ack=False)
    (directory / "cleanup_written").touch()
    # Root kills us before the paused observer can acknowledge this frame.
    connection.recv(4)
    raise RuntimeError("worker survived scheduled wrapper kill")


def wrapper(mode, directory):
    if mode.startswith("receipt-"):
        # Block before spawn; sigwait cannot race installing the wrapper handler.
        signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
        child = subprocess.Popen(
            [sys.executable, __file__, "worker", mode, str(directory)]
        )
        # Impact follows acknowledged worker readiness. Forward only that signal,
        # then exit while the worker retains its independent output and IPC.
        child.send_signal(signal.sigwait({signal.SIGTERM}))
        return
    child = None
    signal.signal(signal.SIGTERM, lambda signum, frame: child.send_signal(signum))
    (directory / "root_pid").write_text(str(os.getpid()))
    child = subprocess.Popen(
        [sys.executable, __file__, "worker", mode, str(directory)],
        start_new_session=mode == "late",
    )
    if mode != "late":
        wait_until(lambda: (directory / "cleanup_written").exists())
        child.kill()
        child.wait(timeout=3)
        (directory / "killed").touch()
        return
    raise SystemExit(child.wait())


def run_case(args, mode):
    with tempfile.TemporaryDirectory(prefix="exitscope-observer-") as tmp:
        directory = Path(tmp)
        graceful = mode != "late"
        profile = {
            "executable": sys.executable,
            "args": [str(Path(__file__).resolve()), "wrapper", mode, tmp],
            "cwd": tmp,
            "scenario": "parent_sigterm" if graceful else "group_sigkill",
            "readiness": "fixture",
            "readiness_ms": 3000,
            "shutdown_ms": 1000 if graceful else 200,
            "output_ms": 2000,
            "contract": {
                "signal_receipt": graceful,
                "cleanup_finished": graceful,
                "root_waits_cleanup": graceful,
                "no_survivors": True,
                "output_eof": True,
            },
        }
        config = directory / "config.json"
        config.write_text(json.dumps(profile))
        process = subprocess.Popen(
            [
                str(args.binary.resolve()),
                "run",
                "--config",
                str(config),
                "--cgroup-parent",
                str(args.cgroup_parent),
                "--json",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        paused = False
        stdout = None
        try:
            wait_until(lambda: (directory / "ready").exists(), process)
            if graceful:
                wait_until(lambda: (directory / "signal_acked").exists(), process)
            else:
                root = int((directory / "root_pid").read_text())
                # Disappearance means the observer has already reaped root.
                wait_until(lambda: not Path(f"/proc/{root}").exists(), process)
            process.send_signal(signal.SIGSTOP)
            paused = True
            if graceful:
                (directory / "continue").touch()
                wait_until(lambda: (directory / "killed").exists(), process)
            else:
                time.sleep(0.25)
                worker_pid = int((directory / "ready").read_text())
                scopes = [p for p in args.cgroup_parent.iterdir() if p.is_dir()]
                assert any(
                    str(worker_pid) in (p / "cgroup.procs").read_text().split()
                    for p in scopes
                )
                wait_until(lambda: (directory / "expired").exists(), process)
                wait_until(
                    lambda: all(
                        "populated 0" in (p / "cgroup.events").read_text()
                        for p in scopes
                    ),
                    process,
                )
            process.send_signal(signal.SIGCONT)
            paused = False
            stdout, stderr = process.communicate(timeout=6)
            (args.reports / f"{mode}.json").write_text(stdout)
            (args.reports / f"{mode}.stderr.txt").write_text(stderr)
            report = json.loads(stdout)
            ids = {f["id"] for f in report["findings"]}
            if graceful:
                assert report["outcome"] == "FAIL", report
                assert process.returncode == 1
                assert "CLEANUP_INCOMPLETE" in ids
                assert report["root_exit"] is not None
                assert report["stdout"]["eof_before_deadline"]
                assert report["stderr"]["eof_before_deadline"]
                assert "ROOT_DID_NOT_WAIT" not in ids
                kinds = [e["kind"] for e in report["events"]]
                assert ("cleanup_started" in kinds) == (mode == "killed")
            else:
                assert report["outcome"] == "UNRESOLVED", report
                assert process.returncode == 2
                assert "CGROUP_DEADLINE_UNOBSERVED" in ids
                assert (
                    report["cgroup_empty_at_ms"]
                    > report["impact_at_ms"] + profile["shutdown_ms"]
                )
            assert not Path(report["cgroup"]).exists()
            assert report["verdict_at_ms"] <= report["cleanup_started_at_ms"]
            print(mode, report["outcome"], sorted(ids), flush=True)
        finally:
            if paused:
                process.send_signal(signal.SIGCONT)
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
            if stdout is None:
                stdout, stderr = process.communicate(timeout=6)
                (args.reports / f"{mode}.json").write_text(stdout)
                (args.reports / f"{mode}.stderr.txt").write_text(stderr)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/debug/exitscope"))
    parser.add_argument("--cgroup-parent", type=Path, required=True)
    parser.add_argument("--reports", type=Path, default=Path("validation/observer"))
    args = parser.parse_args()
    args.reports = args.reports / str(time.time_ns())
    args.reports.mkdir(parents=True, exist_ok=True)
    for mode in ["late", "killed", "truncated"]:
        run_case(args, mode)
    for mode in ["receipt-late", "receipt-timely", "receipt-disabled"]:
        run_receipt_case(args, mode)


def run_receipt_case(args, mode):
    with tempfile.TemporaryDirectory(prefix="exitscope-receipt-") as tmp:
        directory = Path(tmp)
        profile = {
            "executable": sys.executable,
            "args": [str(Path(__file__).resolve()), "wrapper", mode, tmp],
            "cwd": tmp,
            "scenario": "parent_sigterm",
            "readiness": "fixture",
            "readiness_ms": 10000,
            "shutdown_ms": 500,
            "output_ms": 3500,
            "contract": {
                "signal_receipt": mode != "receipt-disabled",
                "cleanup_finished": False,
                "root_waits_cleanup": False,
                "no_survivors": False,
                "output_eof": True,
            },
        }
        config = directory / "config.json"
        config.write_text(json.dumps(profile))
        process = subprocess.Popen(
            [
                str(args.binary.resolve()),
                "run",
                "--config",
                str(config),
                "--cgroup-parent",
                str(args.cgroup_parent),
                "--json",
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        stdout = None
        try:
            wait_until(lambda: (directory / "ready").exists(), process)
            observer_cgroup = Path(f"/proc/{process.pid}/cgroup").read_text()
            stdout, stderr = process.communicate(timeout=15)
            (args.reports / f"{mode}.json").write_text(stdout)
            (args.reports / f"{mode}.stderr.txt").write_text(stderr)
            report = json.loads(stdout)
            ids = {f["id"] for f in report["findings"]}
            expected = "FAIL" if mode == "receipt-late" else "PASS"
            assert report["outcome"] == report["observed_outcome"] == expected, report
            assert process.returncode == (1 if expected == "FAIL" else 0)
            assert ids == ({"SIGNAL_NOT_RECEIVED"} if expected == "FAIL" else set()), (
                report
            )
            receipt = next(
                e for e in report["events"] if e["kind"] == "signal_received"
            )
            deadline = report["impact_at_ms"] + profile["shutdown_ms"]
            assert (receipt["at_ms"] <= deadline) == (mode == "receipt-timely"), report
            assert report["root_exit"]["at_ms"] <= deadline, report
            assert report["stdout"]["eof_before_deadline"]
            assert report["stderr"]["eof_before_deadline"]
            assert Path(report["cgroup"]).name not in observer_cgroup
            assert not Path(report["cgroup"]).exists()
            assert not (
                Path(tempfile.gettempdir()) / Path(report["cgroup"]).name
            ).exists()
            assert report["verdict_at_ms"] <= report["cleanup_started_at_ms"]
            print(mode, report["outcome"], sorted(ids), flush=True)
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
            if stdout is None:
                stdout, stderr = process.communicate(timeout=6)
                (args.reports / f"{mode}.json").write_text(stdout)
                (args.reports / f"{mode}.stderr.txt").write_text(stderr)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] in {"worker", "wrapper"}:
        {"worker": worker, "wrapper": wrapper}[sys.argv[1]](
            sys.argv[2], Path(sys.argv[3])
        )
    else:
        main()
