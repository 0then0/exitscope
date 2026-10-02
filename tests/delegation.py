"""Explicit privileged setup ONLY in a disposable delegated test subtree.

Run as root inside the validation container. Test commands run with UID 65534.
This never modifies systemd or mounts and never touches the global hierarchy.
"""

import argparse
import errno
import json
import os
import subprocess
import time
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("--cgroup-parent", type=Path, required=True)
p.add_argument("--binary", type=Path, default=Path("target/debug/exitscope"))
p.add_argument("--reports", type=Path, default=Path("validation/delegation"))
args = p.parse_args()
args.reports.mkdir(parents=True, exist_ok=True)
base = args.cgroup_parent / f"delegation-{os.getpid()}-{time.time_ns()}"
base.mkdir()
(base / "harness").mkdir()
(base / "runs").mkdir()
uid = 65534
os.chown(base, uid, uid)
os.chown(base / "runs", uid, uid)


def enter():
    (base / "harness/cgroup.procs").write_text(str(os.getpid()))
    os.setgroups([])
    os.setgid(uid)
    os.setuid(uid)


def run(name, expected):
    result = subprocess.run(
        [
            str(args.binary.resolve()),
            "run",
            "--config",
            "examples/parent-sigterm.json",
            "--cgroup-parent",
            str(base / "runs"),
            "--json",
        ],
        preexec_fn=enter,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    report_path = args.reports / f"{time.time_ns()}-{name}.json"
    report_path.write_text(result.stdout)
    report = json.loads(result.stdout)
    print(name, "uid=65534", report["outcome"], flush=True)
    assert report["outcome"] == expected
    if name == "common-ancestor-denied":
        assert report["launch_error"] == {"stage": "cgroup_join", "errno": errno.EACCES}
    assert not [entry for entry in (base / "runs").iterdir() if entry.is_dir()]


try:
    run("common-ancestor-denied", "INFRASTRUCTURE_ERROR")
    for name in ["cgroup.procs", "cgroup.threads", "cgroup.subtree_control"]:
        os.chown(base / name, uid, uid)
    run("delegated-success", "PASS")
finally:
    # No process-kill fallback; commands must have cleaned up their own run children.
    (base / "harness").rmdir()
    (base / "runs").rmdir()
    base.rmdir()
