#!/bin/sh
# Run as an ordinary user inside an explicitly delegated systemd service shell.
# The caller starts the user manager; this script never elevates privileges.
set -eu
test "$(id -u)" -ne 0
binary="$1"
reports="$2"
parent="/sys/fs/cgroup$(awk -F: '$1 == "0" { print $3 }' /proc/self/cgroup)"
mkdir "$parent/harness" "$parent/runs"
printf '%s\n' "$$" > "$parent/harness/cgroup.procs"
mkdir -p "$reports"
{
  cat /etc/os-release
  uname -a
  systemd --version
  id
  systemctl --user show-environment >/dev/null
  cat /proc/self/cgroup
  stat -f -c %T "$parent"
  ls -ld "$parent" "$parent/cgroup.procs"
} > "$reports/environment.txt"
status=0
python3 - --binary "$binary" --cgroup-parent "$parent/runs" \
  --reports "$reports/integration" <<'PY' || status=1
import runpy
import unittest
m = runpy.run_path("tests/integration.py", run_name="systemd_validation")
suite = unittest.TestSuite(m["Integration"](name) for name in [
    "test_correct_forwarding", "test_missing_forwarding_and_pipes"])
result = unittest.TextTestRunner(verbosity=2).run(suite)
raise SystemExit(0 if result.wasSuccessful() else 1)
PY
python3 tests/observer_regressions.py --binary "$binary" \
  --cgroup-parent "$parent/runs" --reports "$reports/observer" || status=1
python3 - "$reports" "$parent/runs" <<'PY' || status=1
import json
import sys
import tempfile
from pathlib import Path
count = 0
for path in Path(sys.argv[1]).rglob("*.json"):
    report = json.loads(path.read_text())
    if report.get("cgroup"):
        assert not Path(report["cgroup"]).exists(), path
        assert not (Path(tempfile.gettempdir()) / Path(report["cgroup"]).name).exists(), path
        assert report["verdict_at_ms"] <= report["cleanup_started_at_ms"], path
        count += 1
assert not [p for p in Path(sys.argv[2]).iterdir() if p.is_dir()]
print(f"{count} systemd run cgroups and private IPC directories removed")
PY
rmdir "$parent/runs"
# The transient service manager owns/removes harness after this shell exits.
exit "$status"
